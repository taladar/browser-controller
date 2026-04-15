//! URL utility functions.

/// Return `url_str` with any embedded `user:password@` credentials removed.
///
/// Used to normalize URLs before comparing them, so that a tab opened with
/// credentials still matches a check against the credential-free URL.
/// Falls back to the original string unchanged if parsing fails (e.g. non-HTTP URLs).
#[must_use]
pub fn strip_url_credentials(url_str: &str) -> String {
    if let Ok(mut u) = url::Url::parse(url_str) {
        // set_username/set_password only fail on cannot-be-a-base URLs (e.g. data:),
        // which cannot carry credentials; the Err(()) is safe to ignore.
        match u.set_username("") {
            Ok(()) | Err(()) => {}
        }
        match u.set_password(None) {
            Ok(()) | Err(()) => {}
        }
        u.to_string()
    } else {
        url_str.to_owned()
    }
}
