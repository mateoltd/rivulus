//! discord.js formatters port (`userMention`, `bold`, ...).
/// Mention a user.
#[must_use]
pub fn user_mention(id: u64) -> String {
    format!("<@{id}>")
}
/// Mention a channel.
#[must_use]
pub fn channel_mention(id: u64) -> String {
    format!("<#{id}>")
}
/// Bold text.
#[must_use]
pub fn bold(s: &str) -> String {
    format!("**{s}**")
}
/// Code block.
#[must_use]
pub fn code_block(lang: &str, s: &str) -> String {
    format!("```{lang}\n{s}```")
}
/// Escape markdown.
#[must_use]
pub fn escape_markdown(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '_' | '~' | '`' | '|' | '\\') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}
