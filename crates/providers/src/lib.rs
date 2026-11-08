//! LLM provider clients.
//!
//! One client, [`OpenAiCompatible`], speaks the OpenAI Chat Completions wire
//! format, which covers DeepSeek, OpenAI, OpenRouter, Groq, Together, Ollama,
//! LM Studio and most other hosted or local providers.

mod sse;

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use sse::{SseLine, parse_sse_line};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a tool call, answering the assistant's `tool_calls`.
    Tool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// The model's earlier reasoning. DeepSeek's thinking mode requires it to
    /// be sent back whenever tools are in play; others never produce it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reasoning_content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Images (data: or https: URLs) sent with a user message.
    #[serde(skip)]
    pub images: Vec<String>,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            reasoning_content: String::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// An assistant turn as sent back in the history (with its reasoning and
    /// any tool calls it made).
    pub fn assistant(content: impl Into<String>, reasoning: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { reasoning_content: reasoning.into(), tool_calls, ..Self::new(Role::Assistant, content) }
    }

    /// A user message with images.
    pub fn user_with_images(content: impl Into<String>, images: Vec<String>) -> Self {
        Self { images, ..Self::new(Role::User, content) }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { tool_call_id: Some(tool_call_id.into()), ..Self::new(Role::Tool, content) }
    }
}

/// A function the model asked to call. `arguments` is the raw JSON text.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl Serialize for ToolCall {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        json!({
            "id": self.id,
            "type": "function",
            "function": { "name": self.name, "arguments": self.arguments },
        })
        .serialize(s)
    }
}

/// A tool the model may call: name, description and a JSON Schema for the
/// arguments.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl Serialize for ToolSpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        })
        .serialize(s)
    }
}

/// How hard a reasoning model thinks, as the user chose it. `Provider` sends
/// nothing, so the provider's own default applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reasoning {
    #[default]
    Provider,
    Off,
    Low,
    Medium,
    High,
    Max,
}

impl Reasoning {
    /// From the level names the engine stores; anything unknown (including
    /// the empty string) means the provider's default.
    pub fn parse(level: &str) -> Self {
        match level {
            "off" => Self::Off,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "max" => Self::Max,
            _ => Self::Provider,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    /// Tools offered to the model for this request.
    pub tools: Vec<ToolSpec>,
    pub reasoning: Reasoning,
    /// Offer the tools but forbid calling them (`tool_choice: "none"`), e.g.
    /// to force a final answer.
    pub no_tool_calls: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    /// DeepSeek reports how much of the prompt was served from its cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_hit_tokens: Option<u32>,
}

/// One streamed piece of the reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    Content(String),
    /// Separate reasoning text ("thinking"), e.g. from `deepseek-reasoner`.
    Reasoning(String),
    /// The model started writing a tool call (its arguments follow).
    ToolCallStarted { index: usize, name: String },
}

/// The finished reply once the stream ends.
#[derive(Debug, Clone, Default)]
pub struct Completion {
    pub content: String,
    pub reasoning: String,
    /// Tools the model wants called before it continues; empty for a final answer.
    pub tool_calls: Vec<ToolCall>,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    #[error("provider sent an error mid-stream: {0}")]
    Stream(String),
    #[error("could not decode provider response: {0}")]
    Decode(String),
}

/// Client for any OpenAI-compatible endpoint.
#[derive(Clone)]
pub struct OpenAiCompatible {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl OpenAiCompatible {
    /// `base_url` is the API root, e.g. `https://api.deepseek.com` or
    /// `https://api.openai.com/v1`; endpoint paths are appended to it.
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("iris/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self { base_url: base_url.into(), api_key, http }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let req = self.http.request(method, self.url(path));
        match &self.api_key {
            Some(key) => req.bearer_auth(key),
            None => req,
        }
    }

    /// Model ids the provider currently offers (`GET /models`).
    pub async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let resp = self
            .request(reqwest::Method::GET, "models")
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        let body: Value = check_status(resp).await?.json().await?;
        let mut ids: Vec<String> = body["data"]
            .as_array()
            .map(|models| {
                models.iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()
            })
            .unwrap_or_default();
        ids.sort();
        Ok(ids)
    }

    /// The account balance, for providers that offer one (DeepSeek:
    /// `GET /user/balance`). Returned as the provider's raw JSON.
    pub async fn balance(&self) -> Result<Value, ProviderError> {
        let resp = self
            .request(reqwest::Method::GET, "user/balance")
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(check_status(resp).await?.json().await?)
    }

    /// Speech to text (`POST /audio/transcriptions`, Whisper-style). `audio`
    /// is a whole recording; `file_name`'s extension tells the format.
    pub async fn transcribe(
        &self,
        model: &str,
        audio: Vec<u8>,
        file_name: &str,
        mime: &str,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        // multipart/form-data by hand: three small fields and the file.
        let boundary = format!("iris{}", uuid_like());
        let mut body = Vec::with_capacity(audio.len() + 512);
        let mut field = |name: &str, value: &str| {
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        };
        field("model", model);
        field("response_format", "json");
        if let Some(lang) = language.filter(|l| !l.is_empty()) {
            field("language", lang);
        }
        let file_name = file_name.replace(['"', '\r', '\n'], "");
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {mime}\r\n\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(&audio);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let resp = self
            .request(reqwest::Method::POST, "audio/transcriptions")
            .header(reqwest::header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(body)
            .timeout(Duration::from_secs(90))
            .send()
            .await?;
        let v: Value = check_status(resp).await?.json().await?;
        Ok(v["text"].as_str().unwrap_or_default().trim().to_owned())
    }

    /// The JSON body of a streamed chat completion request.
    fn chat_body(&self, model: &str, messages: &[ChatMessage], opts: &ChatOptions) -> Value {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        // Messages with images send their content as text + image parts.
        for (i, m) in messages.iter().enumerate().filter(|(_, m)| !m.images.is_empty()) {
            let mut parts = vec![json!({ "type": "text", "text": m.content })];
            parts.extend(m.images.iter().map(|url| json!({ "type": "image_url", "image_url": { "url": url } })));
            body["messages"][i]["content"] = json!(parts);
        }
        if let Some(t) = opts.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(n) = opts.max_tokens {
            body["max_tokens"] = json!(n);
        }
        if !opts.tools.is_empty() {
            body["tools"] = json!(opts.tools);
            if opts.no_tool_calls {
                body["tool_choice"] = json!("none");
            }
        }
        // DeepSeek thinks by default and takes `thinking` plus `reasoning_effort`
        // (low, high or max; medium counts as high). Other providers take the
        // OpenAI-style `reasoning_effort`, which has no "off": that is left out.
        if self.base_url.contains("deepseek") {
            match opts.reasoning {
                Reasoning::Provider => {}
                Reasoning::Off => body["thinking"] = json!({ "type": "disabled" }),
                level => {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!(match level {
                        Reasoning::Low => "low",
                        Reasoning::Max => "max",
                        _ => "high",
                    });
                }
            }
        } else {
            let effort = match opts.reasoning {
                Reasoning::Provider | Reasoning::Off => None,
                Reasoning::Low => Some("low"),
                Reasoning::Medium => Some("medium"),
                Reasoning::High | Reasoning::Max => Some("high"),
            };
            if let Some(effort) = effort {
                body["reasoning_effort"] = json!(effort);
            }
        }
        body
    }

    /// Stream a chat completion, calling `on_delta` for every piece of text as
    /// it arrives. Returns the assembled reply when the stream finishes.
    pub async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        opts: &ChatOptions,
        mut on_delta: impl FnMut(Delta),
    ) -> Result<Completion, ProviderError> {
        let body = self.chat_body(model, messages, opts);
        let resp = self.request(reqwest::Method::POST, "chat/completions").json(&body).send().await?;
        let stream = check_status(resp).await?.bytes_stream();
        read_events(Box::pin(stream), STREAM_IDLE_TIMEOUT, &mut on_delta).await
    }
}

/// How long a reply may go without a single byte (keep-alive comments count)
/// before it is given up on.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// The longest line accepted from a provider; a stream without line breaks
/// would otherwise grow the buffer without end.
const MAX_STREAM_LINE: usize = 4 * 1024 * 1024;

/// Reads a chat-completion event stream into a [`Completion`].
///
/// The reply only counts as complete if the provider said so (`[DONE]` or a
/// `finish_reason`): a connection that simply closes mid-answer is an error,
/// and so is one that goes quiet for longer than `idle`. Whatever text arrived
/// before the failure has already gone through `on_delta`.
async fn read_events<S, B, E>(
    mut stream: S,
    idle: Duration,
    on_delta: &mut impl FnMut(Delta),
) -> Result<Completion, ProviderError>
where
    S: futures_util::Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: Into<ProviderError>,
{
    let mut out = Completion::default();
    let mut buf: Vec<u8> = Vec::new();
    let mut done = false;
    'read: loop {
        let next = tokio::time::timeout(idle, stream.next())
            .await
            .map_err(|_| ProviderError::Stream(format!("the model stopped sending data for {} seconds", idle.as_secs())))?;
        let Some(chunk) = next else { break };
        let bytes = chunk.map_err(|e| -> ProviderError { e.into() })?;
        buf.extend_from_slice(bytes.as_ref());
        // Lines are complete once we see '\n', so decoding them as UTF-8
        // never splits a multi-byte character.
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            match parse_sse_line(&line) {
                SseLine::Data(data) => apply_chunk(&data, &mut out, &mut *on_delta)?,
                SseLine::Done => {
                    done = true;
                    break 'read;
                }
                SseLine::Ignore => {}
            }
        }
        if buf.len() > MAX_STREAM_LINE {
            return Err(ProviderError::Stream("the model sent a line that is far too long".into()));
        }
    }
    if !done && out.finish_reason.is_none() {
        return Err(ProviderError::Stream("the connection closed before the answer was complete".into()));
    }
    Ok(out)
}

/// A multipart boundary unlikely to appear in the audio (clock-based; it only
/// needs to differ from the body's bytes, not be secret).
fn uuid_like() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{nanos:032x}")
}

async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let text = resp.text().await.unwrap_or_default();
    // Most providers wrap errors as {"error": {"message": "..."}}.
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or(text);
    Err(ProviderError::Api { status: status.as_u16(), message })
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    /// DeepSeek's name for streamed reasoning.
    #[serde(default)]
    reasoning_content: Option<String>,
    /// OpenRouter's name for the same thing.
    #[serde(default)]
    reasoning: Option<String>,
    /// Tool calls arrive in pieces, keyed by `index`: the first piece carries
    /// the id and name, later ones append to the JSON arguments.
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Deserialize)]
struct ToolCallDelta {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

fn apply_chunk(
    data: &str,
    out: &mut Completion,
    on_delta: &mut impl FnMut(Delta),
) -> Result<(), ProviderError> {
    let value: Value =
        serde_json::from_str(data).map_err(|e| ProviderError::Decode(format!("{e}: {data}")))?;
    if let Some(err) = value.get("error") {
        let message = err["message"].as_str().map(str::to_owned).unwrap_or_else(|| err.to_string());
        return Err(ProviderError::Stream(message));
    }
    let chunk: Chunk =
        serde_json::from_value(value).map_err(|e| ProviderError::Decode(e.to_string()))?;

    if out.model.is_none() {
        out.model = chunk.model;
    }
    if chunk.usage.is_some() {
        out.usage = chunk.usage;
    }
    for choice in chunk.choices {
        let d = choice.delta;
        if let Some(r) = d.reasoning_content.or(d.reasoning).filter(|s| !s.is_empty()) {
            out.reasoning.push_str(&r);
            on_delta(Delta::Reasoning(r));
        }
        for tc in d.tool_calls.unwrap_or_default() {
            if out.tool_calls.len() <= tc.index {
                out.tool_calls.resize_with(tc.index + 1, ToolCall::default);
            }
            let call = &mut out.tool_calls[tc.index];
            if let Some(id) = tc.id.filter(|s| !s.is_empty()) {
                call.id = id;
            }
            if let Some(f) = tc.function {
                if let Some(name) = f.name.filter(|s| !s.is_empty()) {
                    let first = call.name.is_empty();
                    call.name.push_str(&name);
                    if first {
                        on_delta(Delta::ToolCallStarted { index: tc.index, name: call.name.clone() });
                    }
                }
                if let Some(args) = f.arguments {
                    call.arguments.push_str(&args);
                }
            }
        }
        if let Some(c) = d.content.filter(|s| !s.is_empty()) {
            out.content.push_str(&c);
            on_delta(Delta::Content(c));
        }
        if choice.finish_reason.is_some() {
            out.finish_reason = choice.finish_reason;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&str]) -> (Completion, Vec<Delta>) {
        let mut out = Completion::default();
        let mut deltas = Vec::new();
        for c in chunks {
            apply_chunk(c, &mut out, &mut |d| deltas.push(d)).unwrap();
        }
        (out, deltas)
    }

    #[test]
    fn assembles_content_reasoning_and_usage() {
        let (out, deltas) = run(&[
            r#"{"model":"deepseek-reasoner","choices":[{"delta":{"reasoning_content":"Think"}}]}"#,
            r#"{"choices":[{"delta":{"content":"Hel"}}]}"#,
            r#"{"choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}]}"#,
            r#"{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7,"prompt_cache_hit_tokens":3}}"#,
        ]);
        assert_eq!(out.content, "Hello");
        assert_eq!(out.reasoning, "Think");
        assert_eq!(out.model.as_deref(), Some("deepseek-reasoner"));
        assert_eq!(out.finish_reason.as_deref(), Some("stop"));
        let usage = out.usage.unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (5, 2));
        assert_eq!(usage.prompt_cache_hit_tokens, Some(3));
        assert_eq!(
            deltas,
            vec![
                Delta::Reasoning("Think".into()),
                Delta::Content("Hel".into()),
                Delta::Content("lo".into())
            ]
        );
    }

    fn sse(lines: &[&str]) -> Vec<Result<Vec<u8>, ProviderError>> {
        lines.iter().map(|l| Ok(format!("data: {l}\n\n").into_bytes())).collect()
    }

    #[tokio::test]
    async fn a_complete_stream_is_accepted() {
        let mut got = String::new();
        let chunks = sse(&[
            r#"{"choices":[{"delta":{"content":"Hi"}}]}"#,
            r#"{"choices":[{"delta":{"content":"!"},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]);
        let out = read_events(futures_util::stream::iter(chunks), Duration::from_secs(5), &mut |d| {
            if let Delta::Content(s) = d {
                got.push_str(&s)
            }
        })
        .await
        .unwrap();
        assert_eq!((out.content.as_str(), got.as_str()), ("Hi!", "Hi!"));
        // A provider that sends a finish_reason but no [DONE] is complete too.
        let chunks = sse(&[r#"{"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}"#]);
        assert!(read_events(futures_util::stream::iter(chunks), Duration::from_secs(5), &mut |_| {}).await.is_ok());
    }

    #[tokio::test]
    async fn a_stream_cut_short_is_an_error_not_a_reply() {
        let mut got = String::new();
        let chunks = sse(&[r#"{"choices":[{"delta":{"content":"Half an ans"}}]}"#]);
        let err = read_events(futures_util::stream::iter(chunks), Duration::from_secs(5), &mut |d| {
            if let Delta::Content(s) = d {
                got.push_str(&s)
            }
        })
        .await
        .unwrap_err();
        assert!(err.to_string().contains("closed before the answer was complete"), "{err}");
        assert_eq!(got, "Half an ans", "what arrived was still passed on");
        // An empty stream is no answer either.
        assert!(read_events(futures_util::stream::iter(Vec::<Result<Vec<u8>, ProviderError>>::new()), Duration::from_secs(5), &mut |_| {}).await.is_err());
    }

    #[tokio::test]
    async fn a_silent_stream_times_out() {
        let first = futures_util::stream::iter(sse(&[r#"{"choices":[{"delta":{"content":"Hi"}}]}"#]));
        let stream = first.chain(futures_util::stream::pending());
        let err = read_events(Box::pin(stream), Duration::from_millis(100), &mut |_| {}).await.unwrap_err();
        assert!(err.to_string().contains("stopped sending data"), "{err}");
    }

    #[tokio::test]
    async fn a_stream_without_line_breaks_is_cut_off() {
        let big = vec![Ok::<_, ProviderError>(vec![b'x'; 5 * 1024 * 1024])];
        let err = read_events(futures_util::stream::iter(big), Duration::from_secs(5), &mut |_| {}).await.unwrap_err();
        assert!(err.to_string().contains("too long"), "{err}");
    }

    #[test]
    fn assembles_streamed_tool_calls() {
        let (out, deltas) = run(&[
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"web_search","arguments":""}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"rust\"}"}},{"index":1,"id":"call_2","function":{"name":"get_time","arguments":"{}"}}]}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ]);
        assert_eq!(out.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(
            out.tool_calls,
            vec![
                ToolCall { id: "call_1".into(), name: "web_search".into(), arguments: r#"{"query":"rust"}"#.into() },
                ToolCall { id: "call_2".into(), name: "get_time".into(), arguments: "{}".into() },
            ]
        );
        assert_eq!(
            deltas,
            vec![
                Delta::ToolCallStarted { index: 0, name: "web_search".into() },
                Delta::ToolCallStarted { index: 1, name: "get_time".into() },
            ]
        );
    }

    #[test]
    fn history_messages_serialize_in_openai_shape() {
        let call = ToolCall { id: "c1".into(), name: "get_time".into(), arguments: "{}".into() };
        let asst = serde_json::to_value(ChatMessage::assistant("", "thinking…", vec![call])).unwrap();
        assert_eq!(asst["role"], "assistant");
        assert_eq!(asst["reasoning_content"], "thinking…");
        assert_eq!(asst["tool_calls"][0]["type"], "function");
        assert_eq!(asst["tool_calls"][0]["function"]["name"], "get_time");
        let tool = serde_json::to_value(ChatMessage::tool_result("c1", "12:00")).unwrap();
        assert_eq!(tool["role"], "tool");
        assert_eq!(tool["tool_call_id"], "c1");
        // Plain messages carry no extra fields that other providers might reject.
        let user = serde_json::to_value(ChatMessage::new(Role::User, "hi")).unwrap();
        assert_eq!(user.as_object().unwrap().len(), 2);
    }

    #[test]
    fn openrouter_reasoning_field() {
        let (out, _) = run(&[r#"{"choices":[{"delta":{"reasoning":"hmm"}}]}"#]);
        assert_eq!(out.reasoning, "hmm");
    }

    #[test]
    fn error_chunk_is_reported() {
        let mut out = Completion::default();
        let err = apply_chunk(r#"{"error":{"message":"rate limited"}}"#, &mut out, &mut |_| {})
            .unwrap_err();
        assert!(matches!(err, ProviderError::Stream(m) if m == "rate limited"));
    }

    #[test]
    fn reasoning_effort_maps_per_provider() {
        let body = |base: &str, reasoning| {
            let client = OpenAiCompatible::new(base, None);
            client.chat_body("m", &[], &ChatOptions { reasoning, ..Default::default() })
        };
        let deepseek = "https://api.deepseek.com";
        let other = "https://api.openai.com/v1";

        // Default: nothing is sent, so the provider decides.
        assert!(body(deepseek, Reasoning::Provider).get("thinking").is_none());
        assert!(body(other, Reasoning::Provider).get("reasoning_effort").is_none());

        assert_eq!(body(deepseek, Reasoning::Off)["thinking"]["type"], "disabled");
        assert_eq!(body(deepseek, Reasoning::Medium)["thinking"]["type"], "enabled");
        assert_eq!(body(deepseek, Reasoning::Medium)["reasoning_effort"], "high");
        assert_eq!(body(deepseek, Reasoning::Max)["reasoning_effort"], "max");
        assert_eq!(body(deepseek, Reasoning::Low)["reasoning_effort"], "low");

        assert!(body(other, Reasoning::Off).get("reasoning_effort").is_none());
        assert_eq!(body(other, Reasoning::Low)["reasoning_effort"], "low");
        assert_eq!(body(other, Reasoning::Max)["reasoning_effort"], "high");
        assert!(body(other, Reasoning::High).get("thinking").is_none());

        assert_eq!(Reasoning::parse("max"), Reasoning::Max);
        assert_eq!(Reasoning::parse(""), Reasoning::Provider);
        assert_eq!(Reasoning::parse("bogus"), Reasoning::Provider);
    }

    #[tokio::test]
    async fn transcribes_with_a_multipart_upload() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            // Read until the closing boundary arrives.
            while !String::from_utf8_lossy(&req).trim_end().ends_with("--") {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"text":" hello there "}"#;
            let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            sock.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });
        let client = OpenAiCompatible::new(format!("http://127.0.0.1:{port}/v1"), Some("k".into()));
        let text = client.transcribe("whisper-1", b"RIFFfakewav".to_vec(), "clip.webm", "audio/webm", Some("el")).await.unwrap();
        assert_eq!(text, "hello there");
        let req = server.await.unwrap();
        assert!(req.starts_with("POST /v1/audio/transcriptions"), "{req}");
        assert!(req.contains("authorization: Bearer k") || req.contains("Authorization: Bearer k"));
        assert!(req.contains("name=\"model\"\r\n\r\nwhisper-1"));
        assert!(req.contains("name=\"language\"\r\n\r\nel"));
        assert!(req.contains("filename=\"clip.webm\"\r\nContent-Type: audio/webm\r\n\r\nRIFFfakewav"));
    }
}
