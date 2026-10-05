//! OAuth2 invite helpers.
/// Build a bot invite URL.
///
/// # Example
/// ```
/// let u = common::oauth::generate_invite("123", 8, &[]);
/// assert!(u.contains("123"));
/// ```
#[must_use]
pub fn generate_invite(client_id: &str, permissions: u64, scopes: &[&str]) -> String {
    let mut s = format!("https://discord.com/oauth2/authorize?client_id={client_id}&permissions={permissions}&scope=bot");
    for sc in scopes {
        s.push_str("%20");
        s.push_str(sc);
    }
    s
}
