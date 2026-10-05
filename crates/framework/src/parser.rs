//! Prefix argument parser (quotes respected; no cooldowns/macros in v1).
/// Parsed arguments.
#[derive(Debug, Clone)]
pub struct Args {
    /// Command name.
    pub name: Box<str>,
    /// Rest tokens.
    pub rest: Vec<Box<str>>,
}

/// Split a prefix-less body (`ping a "b c"`) into name + tokens.
///
/// Quotes group tokens; the quotes themselves are stripped. Used by
/// [`parse_args`] after prefix removal and directly for `custom_id`
/// payloads that embed whitespace-separated arguments.
///
/// # Errors
/// Returns [`common::Error::Validation`] on empty input.
pub fn split_command(body: &str) -> Result<Args, common::Error> {
    let mut tokens: Vec<Box<str>> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for c in body.trim().chars() {
        match c {
            '"' => in_q = !in_q,
            ' ' | '\t' if !in_q => {
                if !cur.is_empty() {
                    tokens.push(Box::from(cur.as_str()));
                    cur.clear();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(Box::from(cur.as_str()));
    }
    let mut it = tokens.into_iter();
    let name = it
        .next()
        .ok_or_else(|| common::Error::Validation(Box::from("empty command")))?;
    Ok(Args {
        name,
        rest: it.collect(),
    })
}

/// Parse `!ping a "b c"` into name + tokens.
///
/// # Errors
/// Returns [`common::Error::Validation`] on a missing prefix or empty input.
pub fn parse_args(input: &str, prefix: &str) -> Result<Args, common::Error> {
    let body = input
        .strip_prefix(prefix)
        .ok_or_else(|| common::Error::Validation(Box::from("missing prefix")))?;
    split_command(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted() {
        let a = parse_args("!say \"hello world\" x", "!").unwrap();
        assert_eq!(a.name.as_ref(), "say");
        assert_eq!(a.rest[0].as_ref(), "hello world");
    }

    #[test]
    fn split_command_tokens() {
        let a = split_command("ping a \"b c\"\td").unwrap();
        assert_eq!(a.name.as_ref(), "ping");
        assert_eq!(a.rest.len(), 3);
        assert_eq!(a.rest[1].as_ref(), "b c");
        assert!(split_command("   ").is_err());
        assert!(split_command("").is_err());
        // Same token stream with or without prefix.
        let b = parse_args("!ping a \"b c\"\td", "!").unwrap();
        assert_eq!(a.name, b.name);
        assert_eq!(a.rest, b.rest);
        assert!(parse_args("ping x", "!").is_err());
    }
}
