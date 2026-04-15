//! Window and tab matchers for filtering browser entities by criteria.

use browser_controller_types::{
    CookieStoreId, TabDetails, TabId, TabStatus, WindowId, WindowState, WindowSummary,
};
use regex::Regex;

use crate::Error;

/// Controls behavior when a matcher criterion matches more than one window or tab.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultipleMatchBehavior {
    /// Abort with an error if more than one match is found.
    ///
    /// Zero matches always produce an error regardless of this setting.
    #[default]
    Abort,
    /// Apply the command to every matched window or tab.
    ///
    /// Zero matches still produce an error.
    All,
}

/// Criteria for selecting one or more browser windows.
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every window will match, which will produce an error unless
/// `if_matches_multiple` is set to [`MultipleMatchBehavior::All`].
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag; there is no simpler representation"
)]
#[derive(Debug, Default)]
pub struct WindowMatcher {
    /// Match a window by its exact browser-assigned numeric ID.
    pub window_id: Option<WindowId>,
    /// Match windows whose full title equals this string exactly.
    pub window_title: Option<String>,
    /// Match windows whose title prefix (Firefox `titlePreface`) equals this string exactly.
    pub window_title_prefix: Option<String>,
    /// Match windows whose full title matches this regular expression.
    pub window_title_regex: Option<String>,
    /// Match only windows that currently have input focus.
    pub window_focused: bool,
    /// Match only windows that do not currently have input focus.
    pub window_not_focused: bool,
    /// Match only the most recently focused window.
    pub window_last_focused: bool,
    /// Match only windows that are not the most recently focused.
    pub window_not_last_focused: bool,
    /// Match only windows in this visual state.
    pub window_state: Option<WindowState>,
    /// How to handle a criterion that matches multiple windows.
    pub if_matches_multiple: MultipleMatchBehavior,
}

impl std::fmt::Display for WindowMatcher {
    /// Format the active window criteria as a human-readable string for error messages.
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
        if self.window_focused {
            parts.push("window-focused".to_owned());
        }
        if self.window_not_focused {
            parts.push("window-not-focused".to_owned());
        }
        if self.window_last_focused {
            parts.push("window-last-focused".to_owned());
        }
        if self.window_not_last_focused {
            parts.push("window-not-last-focused".to_owned());
        }
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

/// Criteria for selecting one or more browser tabs.
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every tab in every searched window will match, which will produce an error unless
/// `if_matches_multiple` is set to [`MultipleMatchBehavior::All`].
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag mirroring the boolean fields of TabDetails; there is no simpler representation"
)]
#[derive(Debug, Default)]
pub struct TabMatcher {
    /// Match a tab by its exact browser-assigned numeric ID.
    pub tab_id: Option<TabId>,
    /// Match tabs whose title equals this string exactly.
    pub tab_title: Option<String>,
    /// Match tabs whose title matches this regular expression.
    pub tab_title_regex: Option<String>,
    /// Match tabs whose URL equals this string exactly.
    pub tab_url: Option<String>,
    /// Match tabs whose URL's registered domain equals this string (e.g. `example.com`).
    pub tab_url_domain: Option<String>,
    /// Match tabs whose URL matches this regular expression.
    pub tab_url_regex: Option<String>,
    /// Restrict the search to tabs belonging to the window with this ID.
    pub tab_window_id: Option<WindowId>,
    /// Match only the currently active tab in each window.
    pub tab_active: bool,
    /// Match only tabs that are not the active tab in their window.
    pub tab_not_active: bool,
    /// Match only pinned tabs.
    pub tab_pinned: bool,
    /// Match only unpinned tabs.
    pub tab_not_pinned: bool,
    /// Match only discarded (unloaded from memory) tabs.
    pub tab_discarded: bool,
    /// Match only non-discarded tabs.
    pub tab_not_discarded: bool,
    /// Match only tabs that are currently producing audio.
    pub tab_audible: bool,
    /// Match only tabs that are not currently producing audio.
    pub tab_not_audible: bool,
    /// Match only tabs whose audio is muted.
    pub tab_muted: bool,
    /// Match only tabs whose audio is not muted.
    pub tab_not_muted: bool,
    /// Match only tabs open in a private/incognito window.
    pub tab_incognito: bool,
    /// Match only tabs not open in a private/incognito window.
    pub tab_not_incognito: bool,
    /// Match only tabs that are currently awaiting HTTP basic authentication.
    pub tab_awaiting_auth: bool,
    /// Match only tabs that are not currently awaiting HTTP basic authentication.
    pub tab_not_awaiting_auth: bool,
    /// Match only tabs currently displayed in Reader Mode.
    pub tab_in_reader_mode: bool,
    /// Match only tabs not currently displayed in Reader Mode.
    pub tab_not_in_reader_mode: bool,
    /// Match only tabs with this loading status.
    pub tab_status: Option<TabStatus>,
    /// Match only tabs in a specific Firefox container (by cookie store ID).
    pub tab_cookie_store_id: Option<CookieStoreId>,
    /// Match only tabs in a specific Firefox container (by container name).
    pub tab_container_name: Option<String>,
    /// How to handle a criterion that matches multiple tabs.
    pub if_matches_multiple: MultipleMatchBehavior,
}

impl std::fmt::Display for TabMatcher {
    /// Format the active tab criteria as a human-readable string for error messages.
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
        if let Some(win_id) = self.tab_window_id {
            parts.push(format!("tab-window-id={win_id}"));
        }
        if self.tab_active {
            parts.push("tab-active".to_owned());
        }
        if self.tab_not_active {
            parts.push("tab-not-active".to_owned());
        }
        if self.tab_pinned {
            parts.push("tab-pinned".to_owned());
        }
        if self.tab_not_pinned {
            parts.push("tab-not-pinned".to_owned());
        }
        if self.tab_discarded {
            parts.push("tab-discarded".to_owned());
        }
        if self.tab_not_discarded {
            parts.push("tab-not-discarded".to_owned());
        }
        if self.tab_audible {
            parts.push("tab-audible".to_owned());
        }
        if self.tab_not_audible {
            parts.push("tab-not-audible".to_owned());
        }
        if self.tab_muted {
            parts.push("tab-muted".to_owned());
        }
        if self.tab_not_muted {
            parts.push("tab-not-muted".to_owned());
        }
        if self.tab_incognito {
            parts.push("tab-incognito".to_owned());
        }
        if self.tab_not_incognito {
            parts.push("tab-not-incognito".to_owned());
        }
        if self.tab_awaiting_auth {
            parts.push("tab-awaiting-auth".to_owned());
        }
        if self.tab_not_awaiting_auth {
            parts.push("tab-not-awaiting-auth".to_owned());
        }
        if self.tab_in_reader_mode {
            parts.push("tab-in-reader-mode".to_owned());
        }
        if self.tab_not_in_reader_mode {
            parts.push("tab-not-in-reader-mode".to_owned());
        }
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

/// Apply [`WindowMatcher`] criteria to a list of windows and return the matching IDs.
///
/// All criteria are combined with AND logic. An empty matcher matches every window.
///
/// # Errors
///
/// Returns [`Error::InvalidRegex`] if `window_title_regex` cannot be compiled.
pub fn match_windows(windows: &[WindowSummary], m: &WindowMatcher) -> Result<Vec<WindowId>, Error> {
    let title_regex = m
        .window_title_regex
        .as_deref()
        .map(Regex::new)
        .transpose()?;

    let matched = windows
        .iter()
        .filter(|win| {
            if let Some(id) = m.window_id
                && win.id != id
            {
                return false;
            }
            if let Some(ref title) = m.window_title
                && win.title != *title
            {
                return false;
            }
            if let Some(ref prefix) = m.window_title_prefix
                && win.title_prefix.as_deref() != Some(prefix.as_str())
            {
                return false;
            }
            if let Some(ref re) = title_regex
                && !re.is_match(&win.title)
            {
                return false;
            }
            if m.window_focused && !win.is_focused {
                return false;
            }
            if m.window_not_focused && win.is_focused {
                return false;
            }
            if m.window_last_focused && !win.is_last_focused {
                return false;
            }
            if m.window_not_last_focused && win.is_last_focused {
                return false;
            }
            if let Some(state) = m.window_state
                && win.state != state
            {
                return false;
            }
            true
        })
        .map(|win| win.id)
        .collect();
    Ok(matched)
}

/// Apply [`TabMatcher`] criteria to a list of tabs and return the matching IDs.
///
/// All criteria are combined with AND logic. An empty matcher matches every tab.
///
/// # Errors
///
/// Returns [`Error::InvalidRegex`] if a regex pattern cannot be compiled.
pub fn match_tabs(tabs: &[TabDetails], m: &TabMatcher) -> Result<Vec<TabId>, Error> {
    let title_regex = m.tab_title_regex.as_deref().map(Regex::new).transpose()?;
    let url_regex = m.tab_url_regex.as_deref().map(Regex::new).transpose()?;

    let matched = tabs
        .iter()
        .filter(|tab| {
            if let Some(id) = m.tab_id
                && tab.id != id
            {
                return false;
            }
            if let Some(ref title) = m.tab_title
                && tab.title != *title
            {
                return false;
            }
            if let Some(ref re) = title_regex
                && !re.is_match(&tab.title)
            {
                return false;
            }
            if let Some(ref url) = m.tab_url
                && tab.url != *url
            {
                return false;
            }
            if let Some(ref domain) = m.tab_url_domain {
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
            if let Some(win_id) = m.tab_window_id
                && tab.window_id != win_id
            {
                return false;
            }
            if m.tab_active && !tab.is_active {
                return false;
            }
            if m.tab_not_active && tab.is_active {
                return false;
            }
            if m.tab_pinned && !tab.is_pinned {
                return false;
            }
            if m.tab_not_pinned && tab.is_pinned {
                return false;
            }
            if m.tab_discarded && !tab.is_discarded {
                return false;
            }
            if m.tab_not_discarded && tab.is_discarded {
                return false;
            }
            if m.tab_audible && !tab.is_audible {
                return false;
            }
            if m.tab_not_audible && tab.is_audible {
                return false;
            }
            if m.tab_muted && !tab.is_muted {
                return false;
            }
            if m.tab_not_muted && tab.is_muted {
                return false;
            }
            if m.tab_incognito && !tab.incognito {
                return false;
            }
            if m.tab_not_incognito && tab.incognito {
                return false;
            }
            if m.tab_awaiting_auth && !tab.is_awaiting_auth {
                return false;
            }
            if m.tab_not_awaiting_auth && tab.is_awaiting_auth {
                return false;
            }
            if m.tab_in_reader_mode && !tab.is_in_reader_mode {
                return false;
            }
            if m.tab_not_in_reader_mode && tab.is_in_reader_mode {
                return false;
            }
            if let Some(status) = m.tab_status
                && tab.status != status
            {
                return false;
            }
            if m.tab_cookie_store_id.is_some() && tab.cookie_store_id != m.tab_cookie_store_id {
                return false;
            }
            if let Some(ref name) = m.tab_container_name
                && tab.container_name.as_deref() != Some(name.as_str())
            {
                return false;
            }
            true
        })
        .map(|tab| tab.id)
        .collect();
    Ok(matched)
}
