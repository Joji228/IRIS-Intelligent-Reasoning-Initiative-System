//! Minimal Server-Sent Events line parsing, enough for chat-completion streams.

#[derive(Debug, PartialEq, Eq)]
pub enum SseLine {
    /// A `data:` payload (one JSON chunk).
    Data(String),
    /// The `data: [DONE]` terminator.
    Done,
    /// Blank lines, comments (`: keep-alive`) and fields we don't use.
    Ignore,
}

pub fn parse_sse_line(line: &str) -> SseLine {
    let line = line.trim_end_matches(['\r', '\n']);
    let Some(data) = line.strip_prefix("data:") else {
        return SseLine::Ignore;
    };
    let data = data.trim_start();
    if data == "[DONE]" {
        SseLine::Done
    } else if data.is_empty() {
        SseLine::Ignore
    } else {
        SseLine::Data(data.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines() {
        assert_eq!(parse_sse_line("data: {\"a\":1}\r\n"), SseLine::Data("{\"a\":1}".into()));
        assert_eq!(parse_sse_line("data:{\"a\":1}"), SseLine::Data("{\"a\":1}".into()));
        assert_eq!(parse_sse_line("data: [DONE]\n"), SseLine::Done);
        assert_eq!(parse_sse_line(": keep-alive\n"), SseLine::Ignore);
        assert_eq!(parse_sse_line("\n"), SseLine::Ignore);
        assert_eq!(parse_sse_line("event: ping"), SseLine::Ignore);
    }
}
