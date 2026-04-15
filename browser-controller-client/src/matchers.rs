//! Matchers for filtering browser windows, tabs, and instances by criteria.
//!
//! Use the [`MatchWith`] extension trait to apply a matcher to any collection:
//!
//! ```ignore
//! use browser_controller_client::{MatchWith, WindowMatcher};
//!
//! let matched = windows.match_with(&WindowMatcher::default())?;
//! ```

use browser_controller_types::{
    CookieStoreId, TabDetails, TabId, TabStatus, WindowId, WindowState, WindowSummary,
};
use derive_builder::Builder;
use regex::Regex;

use crate::discovery::DiscoveredInstance;

/// Errors that can occur when matching windows, tabs, or instances.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    /// A regular expression pattern could not be compiled.
    #[error("invalid regex: {0}")]
    InvalidRegex(#[from] regex::Error),
    /// No window matched the given criteria.
    #[error("no window matched: {criteria}")]
    NoMatchingWindow {
        /// Description of the criteria that were used.
        criteria: String,
    },
    /// More than one window matched the criteria and the policy is abort.
    #[error("{count} windows matched: {criteria}")]
    AmbiguousWindow {
        /// Number of windows that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },
    /// No tab matched the given criteria.
    #[error("no tab matched: {criteria}")]
    NoMatchingTab {
        /// Description of the criteria that were used.
        criteria: String,
    },
    /// More than one tab matched the criteria and the policy is abort.
    #[error("{count} tabs matched: {criteria}")]
    AmbiguousTab {
        /// Number of tabs that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },
    /// No instance matched the given criteria.
    #[error("no instance matched: {criteria}")]
    NoMatchingInstance {
        /// Description of the criteria that were used.
        criteria: String,
    },
    /// More than one instance matched the criteria and the policy is abort.
    #[error("{count} instances matched: {criteria}")]
    AmbiguousInstance {
        /// Number of instances that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },
}

// ---------------------------------------------------------------------------
// BooleanCondition
// ---------------------------------------------------------------------------

/// A two-valued filter condition for boolean properties.
///
/// Used as `Option<BooleanCondition>` where `None` means "don't filter",
/// `Some(Is)` means "must be true", and `Some(IsNot)` means "must be false".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanCondition {
    /// The property must be `true`.
    Is,
    /// The property must be `false`.
    IsNot,
}

/// Test whether a boolean value satisfies an optional condition.
const fn bool_matches(condition: Option<BooleanCondition>, value: bool) -> bool {
    match condition {
        None => true,
        Some(BooleanCondition::Is) => value,
        Some(BooleanCondition::IsNot) => !value,
    }
}

/// Push a human-readable representation of a boolean condition to a list.
fn push_bool_condition(parts: &mut Vec<String>, name: &str, cond: Option<BooleanCondition>) {
    match cond {
        Some(BooleanCondition::Is) => parts.push(name.to_owned()),
        Some(BooleanCondition::IsNot) => parts.push(format!("not-{name}")),
        None => {}
    }
}

// ---------------------------------------------------------------------------
// BrowserKind
// ---------------------------------------------------------------------------

/// Known browser types.
///
/// Used in [`InstanceMatcher`] to match by browser kind rather than free-text
/// name. Annotated `#[non_exhaustive]` since new browsers may be added.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    /// Mozilla Firefox.
    Firefox,
    /// LibreWolf (privacy-focused Firefox fork).
    Librewolf,
    /// Waterfox (performance-focused Firefox fork).
    Waterfox,
    /// Google Chrome.
    Chrome,
    /// Chromium (open-source Chrome base).
    Chromium,
    /// Brave Browser.
    Brave,
    /// Microsoft Edge.
    Edge,
}

impl BrowserKind {
    /// Test whether this kind matches a `BrowserInfo::browser_name` string
    /// (case-insensitive).
    #[must_use]
    pub fn matches_browser_name(self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        match self {
            Self::Firefox => lower == "firefox",
            Self::Librewolf => lower == "librewolf",
            Self::Waterfox => lower == "waterfox",
            Self::Chrome => lower == "chrome" || lower == "google chrome",
            Self::Chromium => lower == "chromium",
            Self::Brave => lower == "brave" || lower == "brave browser",
            Self::Edge => lower == "edge" || lower == "microsoft edge",
        }
    }
}

impl std::fmt::Display for BrowserKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Firefox => write!(f, "Firefox"),
            Self::Librewolf => write!(f, "Librewolf"),
            Self::Waterfox => write!(f, "Waterfox"),
            Self::Chrome => write!(f, "Chrome"),
            Self::Chromium => write!(f, "Chromium"),
            Self::Brave => write!(f, "Brave"),
            Self::Edge => write!(f, "Edge"),
        }
    }
}

impl std::str::FromStr for BrowserKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "firefox" => Ok(Self::Firefox),
            "librewolf" => Ok(Self::Librewolf),
            "waterfox" => Ok(Self::Waterfox),
            "chrome" => Ok(Self::Chrome),
            "chromium" => Ok(Self::Chromium),
            "brave" => Ok(Self::Brave),
            "edge" => Ok(Self::Edge),
            _ => Err(format!("unknown browser kind: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// MultipleMatchBehavior
// ---------------------------------------------------------------------------

/// Controls behavior when a matcher criterion matches more than one entity.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultipleMatchBehavior {
    /// Abort with an error if more than one match is found.
    ///
    /// Zero matches always produce an error regardless of this setting.
    #[default]
    Abort,
    /// Apply the command to every matched entity.
    ///
    /// Zero matches still produce an error.
    All,
}

// ---------------------------------------------------------------------------
// MatchWith trait
// ---------------------------------------------------------------------------

/// Extension trait for filtering a collection with a matcher.
///
/// Implemented on `IntoIterator` types so you can call `.match_with(&matcher)`
/// on slices, vectors, and iterator adapters.
pub trait MatchWith<'a, M> {
    /// The element type of the collection.
    type Item: 'a;
    /// Apply the matcher and return references to all matching items.
    ///
    /// # Errors
    ///
    /// Returns an error if a regex pattern in the matcher cannot be compiled.
    fn match_with(self, matcher: &M) -> Result<Vec<&'a Self::Item>, MatchError>;
}

// ---------------------------------------------------------------------------
// WindowMatcher
// ---------------------------------------------------------------------------

/// Criteria for selecting one or more browser windows.
///
/// All provided criteria are combined with AND logic. Construct via
/// [`WindowMatcherBuilder`] or [`Default::default()`] (matches everything).
#[derive(Debug, Default, Builder)]
#[builder(setter(into, strip_option), default)]
pub struct WindowMatcher {
    /// Match a window by its exact browser-assigned numeric ID.
    pub(crate) window_id: Option<WindowId>,
    /// Match windows whose full title equals this string exactly.
    pub(crate) window_title: Option<String>,
    /// Match windows whose title prefix (Firefox `titlePreface`) equals this string.
    pub(crate) window_title_prefix: Option<String>,
    /// Match windows whose full title matches this regular expression.
    pub(crate) window_title_regex: Option<String>,
    /// Filter by window focus state.
    pub(crate) window_focused: Option<BooleanCondition>,
    /// Filter by last-focused state.
    pub(crate) window_last_focused: Option<BooleanCondition>,
    /// Match only windows in this visual state.
    pub(crate) window_state: Option<WindowState>,
    /// How to handle a criterion that matches multiple windows.
    #[builder(setter(skip = false, into = false, strip_option = false))]
    pub(crate) if_matches_multiple: MultipleMatchBehavior,
}

impl WindowMatcher {
    /// Create a builder for constructing a `WindowMatcher`.
    #[must_use]
    pub fn builder() -> WindowMatcherBuilder {
        WindowMatcherBuilder::default()
    }

    /// How to handle multiple matches.
    #[must_use]
    pub const fn if_matches_multiple(&self) -> MultipleMatchBehavior {
        self.if_matches_multiple
    }
}

impl std::fmt::Display for WindowMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(id) = self.window_id {
            parts.push(format!("window-id={id}"));
        }
        if let Some(ref title) = self.window_title {
            parts.push(format!("window-title={title:?}"));
        }
        if let Some(ref prefix) = self.window_title_prefix {
            parts.push(format!("window-title-prefix={prefix:?}"));
        }
        if let Some(ref regex) = self.window_title_regex {
            parts.push(format!("window-title-regex={regex:?}"));
        }
        push_bool_condition(&mut parts, "window-focused", self.window_focused);
        push_bool_condition(&mut parts, "window-last-focused", self.window_last_focused);
        if let Some(state) = self.window_state {
            parts.push(format!("window-state={state:?}"));
        }
        if parts.is_empty() {
            write!(f, "(all windows)")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

impl<'a, I> MatchWith<'a, WindowMatcher> for I
where
    I: IntoIterator<Item = &'a WindowSummary>,
{
    type Item = WindowSummary;

    fn match_with(self, matcher: &WindowMatcher) -> Result<Vec<&'a WindowSummary>, MatchError> {
        let title_regex = matcher
            .window_title_regex
            .as_deref()
            .map(Regex::new)
            .transpose()?;

        let matched = self
            .into_iter()
            .filter(|win| {
                if let Some(id) = matcher.window_id
                    && win.id != id
                {
                    return false;
                }
                if let Some(ref title) = matcher.window_title
                    && win.title != *title
                {
                    return false;
                }
                if let Some(ref prefix) = matcher.window_title_prefix
                    && win.title_prefix.as_deref() != Some(prefix.as_str())
                {
                    return false;
                }
                if let Some(ref re) = title_regex
                    && !re.is_match(&win.title)
                {
                    return false;
                }
                if !bool_matches(matcher.window_focused, win.is_focused) {
                    return false;
                }
                if !bool_matches(matcher.window_last_focused, win.is_last_focused) {
                    return false;
                }
                if let Some(state) = matcher.window_state
                    && win.state != state
                {
                    return false;
                }
                true
            })
            .collect();
        Ok(matched)
    }
}

// ---------------------------------------------------------------------------
// TabMatcher
// ---------------------------------------------------------------------------

/// Criteria for selecting one or more browser tabs.
///
/// All provided criteria are combined with AND logic. Construct via
/// [`TabMatcherBuilder`] or [`Default::default()`] (matches everything).
#[derive(Debug, Default, Builder)]
#[builder(setter(into, strip_option), default)]
pub struct TabMatcher {
    /// Match a tab by its exact browser-assigned numeric ID.
    pub(crate) tab_id: Option<TabId>,
    /// Match tabs whose title equals this string exactly.
    pub(crate) tab_title: Option<String>,
    /// Match tabs whose title matches this regular expression.
    pub(crate) tab_title_regex: Option<String>,
    /// Match tabs whose URL equals this string exactly.
    pub(crate) tab_url: Option<String>,
    /// Match tabs whose URL's registered domain equals this string.
    pub(crate) tab_url_domain: Option<String>,
    /// Match tabs whose URL matches this regular expression.
    pub(crate) tab_url_regex: Option<String>,
    /// Filter by active/inactive state.
    pub(crate) tab_active: Option<BooleanCondition>,
    /// Filter by pinned state.
    pub(crate) tab_pinned: Option<BooleanCondition>,
    /// Filter by discarded state.
    pub(crate) tab_discarded: Option<BooleanCondition>,
    /// Filter by audible state.
    pub(crate) tab_audible: Option<BooleanCondition>,
    /// Filter by muted state.
    pub(crate) tab_muted: Option<BooleanCondition>,
    /// Filter by incognito state.
    pub(crate) tab_incognito: Option<BooleanCondition>,
    /// Filter by awaiting-auth state.
    pub(crate) tab_awaiting_auth: Option<BooleanCondition>,
    /// Filter by reader-mode state.
    pub(crate) tab_in_reader_mode: Option<BooleanCondition>,
    /// Filter by has-attention state.
    pub(crate) tab_has_attention: Option<BooleanCondition>,
    /// Match only tabs with this loading status.
    pub(crate) tab_status: Option<TabStatus>,
    /// Match only tabs in a specific Firefox container (by cookie store ID).
    pub(crate) tab_cookie_store_id: Option<CookieStoreId>,
    /// Match only tabs in a specific Firefox container (by container name).
    pub(crate) tab_container_name: Option<String>,
    /// How to handle a criterion that matches multiple tabs.
    #[builder(setter(skip = false, into = false, strip_option = false))]
    pub(crate) if_matches_multiple: MultipleMatchBehavior,
}

impl TabMatcher {
    /// Create a builder for constructing a `TabMatcher`.
    #[must_use]
    pub fn builder() -> TabMatcherBuilder {
        TabMatcherBuilder::default()
    }

    /// How to handle multiple matches.
    #[must_use]
    pub const fn if_matches_multiple(&self) -> MultipleMatchBehavior {
        self.if_matches_multiple
    }
}

impl std::fmt::Display for TabMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(id) = self.tab_id {
            parts.push(format!("tab-id={id}"));
        }
        if let Some(ref title) = self.tab_title {
            parts.push(format!("tab-title={title:?}"));
        }
        if let Some(ref regex) = self.tab_title_regex {
            parts.push(format!("tab-title-regex={regex:?}"));
        }
        if let Some(ref url) = self.tab_url {
            parts.push(format!("tab-url={url:?}"));
        }
        if let Some(ref domain) = self.tab_url_domain {
            parts.push(format!("tab-url-domain={domain:?}"));
        }
        if let Some(ref regex) = self.tab_url_regex {
            parts.push(format!("tab-url-regex={regex:?}"));
        }
        push_bool_condition(&mut parts, "tab-active", self.tab_active);
        push_bool_condition(&mut parts, "tab-pinned", self.tab_pinned);
        push_bool_condition(&mut parts, "tab-discarded", self.tab_discarded);
        push_bool_condition(&mut parts, "tab-audible", self.tab_audible);
        push_bool_condition(&mut parts, "tab-muted", self.tab_muted);
        push_bool_condition(&mut parts, "tab-incognito", self.tab_incognito);
        push_bool_condition(&mut parts, "tab-awaiting-auth", self.tab_awaiting_auth);
        push_bool_condition(&mut parts, "tab-in-reader-mode", self.tab_in_reader_mode);
        push_bool_condition(&mut parts, "tab-has-attention", self.tab_has_attention);
        if let Some(status) = self.tab_status {
            parts.push(format!("tab-status={status:?}"));
        }
        if let Some(ref id) = self.tab_cookie_store_id {
            parts.push(format!("tab-cookie-store-id={:?}", id.0));
        }
        if let Some(ref name) = self.tab_container_name {
            parts.push(format!("tab-container-name={name:?}"));
        }
        if parts.is_empty() {
            write!(f, "(all tabs)")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

impl<'a, I> MatchWith<'a, TabMatcher> for I
where
    I: IntoIterator<Item = &'a TabDetails>,
{
    type Item = TabDetails;

    fn match_with(self, matcher: &TabMatcher) -> Result<Vec<&'a TabDetails>, MatchError> {
        let title_regex = matcher
            .tab_title_regex
            .as_deref()
            .map(Regex::new)
            .transpose()?;
        let url_regex = matcher
            .tab_url_regex
            .as_deref()
            .map(Regex::new)
            .transpose()?;

        let matched = self
            .into_iter()
            .filter(|tab| {
                if let Some(id) = matcher.tab_id
                    && tab.id != id
                {
                    return false;
                }
                if let Some(ref title) = matcher.tab_title
                    && tab.title != *title
                {
                    return false;
                }
                if let Some(ref re) = title_regex
                    && !re.is_match(&tab.title)
                {
                    return false;
                }
                if let Some(ref url) = matcher.tab_url
                    && tab.url != *url
                {
                    return false;
                }
                if let Some(ref domain) = matcher.tab_url_domain {
                    let tab_domain = url::Url::parse(&tab.url)
                        .ok()
                        .and_then(|u| u.domain().map(|s| s.to_owned()))
                        .unwrap_or_default();
                    if tab_domain != *domain {
                        return false;
                    }
                }
                if let Some(ref re) = url_regex
                    && !re.is_match(&tab.url)
                {
                    return false;
                }
                if !bool_matches(matcher.tab_active, tab.is_active) {
                    return false;
                }
                if !bool_matches(matcher.tab_pinned, tab.is_pinned) {
                    return false;
                }
                if !bool_matches(matcher.tab_discarded, tab.is_discarded) {
                    return false;
                }
                if !bool_matches(matcher.tab_audible, tab.is_audible) {
                    return false;
                }
                if !bool_matches(matcher.tab_muted, tab.is_muted) {
                    return false;
                }
                if !bool_matches(matcher.tab_incognito, tab.incognito) {
                    return false;
                }
                if !bool_matches(matcher.tab_awaiting_auth, tab.is_awaiting_auth) {
                    return false;
                }
                if !bool_matches(matcher.tab_in_reader_mode, tab.is_in_reader_mode) {
                    return false;
                }
                if !bool_matches(matcher.tab_has_attention, tab.has_attention) {
                    return false;
                }
                if let Some(status) = matcher.tab_status
                    && tab.status != status
                {
                    return false;
                }
                if matcher.tab_cookie_store_id.is_some()
                    && tab.cookie_store_id != matcher.tab_cookie_store_id
                {
                    return false;
                }
                if let Some(ref name) = matcher.tab_container_name
                    && tab.container_name.as_deref() != Some(name.as_str())
                {
                    return false;
                }
                true
            })
            .collect();
        Ok(matched)
    }
}

// ---------------------------------------------------------------------------
// InstanceMatcher
// ---------------------------------------------------------------------------

/// Criteria for selecting one or more browser instances.
///
/// All provided criteria are combined with AND logic. Construct via
/// [`InstanceMatcherBuilder`], [`Default::default()`] (matches everything),
/// or [`From<&str>`] (PID or browser name substring, for CLI compatibility).
#[derive(Debug, Default, Builder)]
#[builder(setter(into, strip_option), default)]
pub struct InstanceMatcher {
    /// Match by exact PID.
    pub(crate) pid: Option<u32>,
    /// Match by browser kind.
    pub(crate) browser_kind: Option<BrowserKind>,
    /// Match by case-insensitive substring of browser name.
    pub(crate) browser_name_substring: Option<String>,
    /// Match by regex on browser name.
    pub(crate) browser_name_regex: Option<String>,
    /// Match by exact profile ID.
    pub(crate) profile_id: Option<String>,
    /// How to handle a criterion that matches multiple instances.
    #[builder(setter(skip = false, into = false, strip_option = false))]
    pub(crate) if_matches_multiple: MultipleMatchBehavior,
}

impl InstanceMatcher {
    /// Create a builder for constructing an `InstanceMatcher`.
    #[must_use]
    pub fn builder() -> InstanceMatcherBuilder {
        InstanceMatcherBuilder::default()
    }

    /// How to handle multiple matches.
    #[must_use]
    pub const fn if_matches_multiple(&self) -> MultipleMatchBehavior {
        self.if_matches_multiple
    }
}

impl std::fmt::Display for InstanceMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(pid) = self.pid {
            parts.push(format!("pid={pid}"));
        }
        if let Some(kind) = self.browser_kind {
            parts.push(format!("browser-kind={kind}"));
        }
        if let Some(ref name) = self.browser_name_substring {
            parts.push(format!("browser-name={name:?}"));
        }
        if let Some(ref regex) = self.browser_name_regex {
            parts.push(format!("browser-name-regex={regex:?}"));
        }
        if let Some(ref profile) = self.profile_id {
            parts.push(format!("profile-id={profile:?}"));
        }
        if parts.is_empty() {
            write!(f, "(any instance)")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

/// Parse a selector string into an `InstanceMatcher`.
///
/// If the string is numeric, it is treated as a PID. Otherwise it is treated
/// as a case-insensitive browser name substring. This preserves the behavior
/// of the former `select_instance` function.
impl From<&str> for InstanceMatcher {
    fn from(s: &str) -> Self {
        if let Ok(pid) = s.parse::<u32>() {
            Self {
                pid: Some(pid),
                ..Self::default()
            }
        } else {
            Self {
                browser_name_substring: Some(s.to_owned()),
                ..Self::default()
            }
        }
    }
}

impl From<String> for InstanceMatcher {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl<'a, I> MatchWith<'a, InstanceMatcher> for I
where
    I: IntoIterator<Item = &'a DiscoveredInstance>,
{
    type Item = DiscoveredInstance;

    fn match_with(
        self,
        matcher: &InstanceMatcher,
    ) -> Result<Vec<&'a DiscoveredInstance>, MatchError> {
        let name_regex = matcher
            .browser_name_regex
            .as_deref()
            .map(Regex::new)
            .transpose()?;

        let matched = self
            .into_iter()
            .filter(|inst| {
                if let Some(pid) = matcher.pid
                    && inst.info.pid != pid
                {
                    return false;
                }
                if let Some(kind) = matcher.browser_kind
                    && !kind.matches_browser_name(&inst.info.browser_name)
                {
                    return false;
                }
                if let Some(ref substring) = matcher.browser_name_substring {
                    let name_lower = inst.info.browser_name.to_ascii_lowercase();
                    if !name_lower.contains(&substring.to_ascii_lowercase()) {
                        return false;
                    }
                }
                if let Some(ref re) = name_regex
                    && !re.is_match(&inst.info.browser_name)
                {
                    return false;
                }
                if let Some(ref profile) = matcher.profile_id
                    && inst.info.profile_id.as_deref() != Some(profile.as_str())
                {
                    return false;
                }
                true
            })
            .collect();
        Ok(matched)
    }
}
