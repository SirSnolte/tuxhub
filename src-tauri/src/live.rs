//! Live interface (integration level 3).
//!
//! A running application listens on `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`. tuxhub
//! connects, sends `hello` and then requests the schema, the values and
//! changes, one JSON object per line in the form of the RPC protocol of
//! level 2. The application describes its settings itself; everything it sends
//! is external input and is checked here before it reaches the interface.

use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{oneshot, watch};

/// Protocol version this tuxhub speaks.
pub const PROTOCOL: u64 = 1;
/// Longest line the application may send, without the line break.
const MAX_LINE: usize = 1 << 20;
/// Time the application has to answer a request.
const TIMEOUT: Duration = Duration::from_secs(5);
/// Longest error text of the application that is shown.
const MAX_ERROR: usize = 500;

const MAX_SECTIONS: usize = 32;
const MAX_FIELDS: usize = 200;
const MAX_ACTIONS: usize = 32;
const MAX_OPTIONS: usize = 200;
const MAX_KEY: usize = 64;
const MAX_LABEL: usize = 120;
const MAX_HELP: usize = 500;
const MAX_OPTION_VALUE: usize = 500;
/// Text values without their own `max_length`.
const MAX_TEXT: usize = 65_536;

/// An error the interface shows: `code` selects the text, `message` carries
/// the details (the application's own text for `rejected`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveError {
    pub code: &'static str,
    pub message: String,
}

impl LiveError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Toggle,
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        slider: bool,
    },
    Select {
        options: Vec<Choice>,
    },
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_length: Option<usize>,
        multiline: bool,
    },
    Color {
        alpha: bool,
    },
    /// A type this tuxhub does not know: shown as a notice.
    Unsupported {
        declared: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    pub disabled: bool,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Section {
    pub id: String,
    pub label: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub danger: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

/// A field or action that was left out, and why.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Invalid {
    pub key: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Schema {
    pub sections: Vec<Section>,
    pub actions: Vec<Action>,
    pub invalid: Vec<Invalid>,
}

impl Schema {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.sections
            .iter()
            .flat_map(|s| &s.fields)
            .find(|f| f.key == key)
    }
}

/// Checks the schema an application sent. Invalid fields and actions are left
/// out and listed in `invalid`; the rest stays usable.
pub fn parse_schema(v: &Value) -> Result<Schema, LiveError> {
    let bad = |m: &str| LiveError::new("invalid-answer", format!("schema: {m}"));
    let o = v.as_object().ok_or_else(|| bad("not an object"))?;
    let sections = list(o, "sections").map_err(|e| bad(&e))?;
    let actions = list(o, "actions").map_err(|e| bad(&e))?;

    let mut out = Schema::default();
    let mut keys = std::collections::HashSet::new();
    for (i, sv) in sections.iter().enumerate() {
        if i == MAX_SECTIONS {
            out.invalid.push(Invalid {
                key: format!("#{i}"),
                reason: format!("more than {MAX_SECTIONS} sections"),
            });
            break;
        }
        let (mut section, fields) = match parse_section(sv) {
            Ok(x) => x,
            Err(reason) => {
                out.invalid.push(Invalid {
                    key: name_of(sv, "id", i),
                    reason,
                });
                continue;
            }
        };
        for (j, fv) in fields.iter().enumerate() {
            if j == MAX_FIELDS {
                out.invalid.push(Invalid {
                    key: format!("#{j}"),
                    reason: format!("more than {MAX_FIELDS} fields in {}", section.id),
                });
                break;
            }
            match parse_field(fv) {
                Ok(f) if !keys.insert(f.key.clone()) => out.invalid.push(Invalid {
                    key: f.key,
                    reason: "duplicate key".into(),
                }),
                Ok(f) => section.fields.push(f),
                Err(reason) => out.invalid.push(Invalid {
                    key: name_of(fv, "key", j),
                    reason,
                }),
            }
        }
        out.sections.push(section);
    }

    for (i, av) in actions.iter().enumerate() {
        if i == MAX_ACTIONS {
            out.invalid.push(Invalid {
                key: format!("#{i}"),
                reason: format!("more than {MAX_ACTIONS} actions"),
            });
            break;
        }
        match parse_action(av) {
            Ok(a) if out.actions.iter().any(|b| b.id == a.id) => out.invalid.push(Invalid {
                key: a.id,
                reason: "duplicate action".into(),
            }),
            Ok(a) => out.actions.push(a),
            Err(reason) => out.invalid.push(Invalid {
                key: name_of(av, "id", i),
                reason,
            }),
        }
    }
    Ok(out)
}

/// An optional list.
fn list<'a>(o: &'a Map<String, Value>, name: &str) -> Result<&'a [Value], String> {
    match o.get(name) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(format!("`{name}` is not a list")),
    }
}

/// Name of an entry for `invalid`: its key or id if it is a string, otherwise
/// its position.
fn name_of(v: &Value, name: &str, i: usize) -> String {
    match v.get(name).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => s.chars().take(MAX_KEY + 1).collect(),
        _ => format!("#{i}"),
    }
}

/// Keys and ids: `[A-Za-z0-9_.-]`, at most 64 characters.
fn valid_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_KEY
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn ident(o: &Map<String, Value>, name: &str) -> Result<String, String> {
    match o.get(name) {
        Some(Value::String(s)) if valid_key(s) => Ok(s.clone()),
        Some(_) => Err(format!(
            "`{name}` may only contain letters, digits, `_`, `.` and `-` (at most {MAX_KEY})"
        )),
        None => Err(format!("`{name}` is missing")),
    }
}

/// An optional string of at most `max` characters.
fn text(o: &Map<String, Value>, name: &str, max: usize) -> Result<Option<String>, String> {
    match o.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.chars().count() <= max => Ok(Some(s.clone())),
        Some(Value::String(_)) => Err(format!("`{name}` is longer than {max} characters")),
        Some(_) => Err(format!("`{name}` is not a string")),
    }
}

fn required(o: &Map<String, Value>, name: &str, max: usize) -> Result<String, String> {
    text(o, name, max)?.ok_or_else(|| format!("`{name}` is missing"))
}

fn flag(o: &Map<String, Value>, name: &str) -> Result<bool, String> {
    match o.get(name) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("`{name}` is not true or false")),
    }
}

fn number(o: &Map<String, Value>, name: &str) -> Result<Option<f64>, String> {
    match o.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .map(Some)
            .ok_or_else(|| format!("`{name}` is not a number")),
    }
}

fn parse_section(v: &Value) -> Result<(Section, &[Value]), String> {
    let o = v.as_object().ok_or("not an object")?;
    let section = Section {
        id: ident(o, "id")?,
        label: required(o, "label", MAX_LABEL)?,
        fields: Vec::new(),
    };
    Ok((section, list(o, "fields")?))
}

fn parse_field(v: &Value) -> Result<Field, String> {
    let o = v.as_object().ok_or("not an object")?;
    let key = ident(o, "key")?;
    let label = required(o, "label", MAX_LABEL)?;
    let help = text(o, "help", MAX_HELP)?;
    let disabled = flag(o, "disabled")?;
    let ty = match o.get("type") {
        Some(Value::String(t)) => t.as_str(),
        Some(_) => return Err("`type` is not a string".into()),
        None => return Err("`type` is missing".into()),
    };
    let kind = match ty {
        "toggle" => Kind::Toggle,
        "number" => {
            let (min, max) = (number(o, "min")?, number(o, "max")?);
            if let (Some(a), Some(b)) = (min, max)
                && a > b
            {
                return Err("`min` is greater than `max`".into());
            }
            let step = number(o, "step")?;
            if step.is_some_and(|s| s <= 0.0) {
                return Err("`step` is not positive".into());
            }
            Kind::Number {
                min,
                max,
                step,
                unit: text(o, "unit", MAX_LABEL)?,
                slider: flag(o, "slider")?,
            }
        }
        "select" => {
            let entries = list(o, "options")?;
            if entries.is_empty() || entries.len() > MAX_OPTIONS {
                return Err(format!("`options` needs 1 to {MAX_OPTIONS} entries"));
            }
            let mut options: Vec<Choice> = Vec::with_capacity(entries.len());
            for ov in entries {
                let oo = ov.as_object().ok_or("an option is not an object")?;
                let value = required(oo, "value", MAX_OPTION_VALUE)?;
                let label = required(oo, "label", MAX_LABEL)?;
                if options.iter().any(|c| c.value == value) {
                    return Err(format!("option {value:?} appears twice"));
                }
                options.push(Choice { value, label });
            }
            Kind::Select { options }
        }
        "text" => {
            let max_length = match o.get("max_length") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_u64()
                        .filter(|n| (1..=MAX_TEXT as u64).contains(n))
                        .ok_or(format!("`max_length` must be 1 to {MAX_TEXT}"))?
                        as usize,
                ),
            };
            Kind::Text {
                placeholder: text(o, "placeholder", MAX_LABEL)?,
                max_length,
                multiline: flag(o, "multiline")?,
            }
        }
        "color" => Kind::Color {
            alpha: flag(o, "alpha")?,
        },
        other => Kind::Unsupported {
            declared: other.chars().take(MAX_KEY).collect(),
        },
    };
    // A default that does not fit is ignored, not the field.
    let default = o.get("default").filter(|d| fits(&kind, d)).cloned();
    Ok(Field {
        key,
        label,
        help,
        default,
        disabled,
        kind,
    })
}

fn parse_action(v: &Value) -> Result<Action, String> {
    let o = v.as_object().ok_or("not an object")?;
    Ok(Action {
        id: ident(o, "id")?,
        label: required(o, "label", MAX_LABEL)?,
        danger: flag(o, "danger")?,
        confirm: text(o, "confirm", MAX_HELP)?,
    })
}

/// `#rrggbb`, or `#rrggbbaa` when the field has `alpha`.
fn color(s: &str, alpha: bool) -> bool {
    s.strip_prefix('#').is_some_and(|hex| {
        (hex.len() == 6 || (alpha && hex.len() == 8)) && hex.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Whether `v` has the type of the field. Range, options and length of its
/// own values are left to the application.
fn fits(kind: &Kind, v: &Value) -> bool {
    match kind {
        Kind::Toggle => v.is_boolean(),
        Kind::Number { .. } => v.as_f64().is_some_and(f64::is_finite),
        Kind::Select { .. } => v
            .as_str()
            .is_some_and(|s| s.chars().count() <= MAX_OPTION_VALUE),
        Kind::Text { .. } => v.as_str().is_some_and(|s| s.chars().count() <= MAX_TEXT),
        Kind::Color { alpha } => v.as_str().is_some_and(|s| color(s, *alpha)),
        Kind::Unsupported { .. } => false,
    }
}

/// Checks a value before it is sent with `set`.
pub fn check_set(field: &Field, v: &Value) -> Result<Value, LiveError> {
    let refuse = |code, m: String| Err(LiveError::new(code, m));
    if field.disabled {
        return refuse("disabled", format!("{} cannot be changed", field.key));
    }
    if let Kind::Unsupported { declared } = &field.kind {
        return refuse("type", format!("type {declared:?} is not supported"));
    }
    if !fits(&field.kind, v) {
        if matches!(field.kind, Kind::Color { .. }) && v.is_string() {
            return refuse("color", format!("{v} is not a color"));
        }
        return refuse("type", format!("{v} does not fit {}", field.key));
    }
    match &field.kind {
        Kind::Number { min, max, .. } => {
            let n = v.as_f64().unwrap_or_default();
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                let (a, b) = (
                    min.unwrap_or(f64::NEG_INFINITY),
                    max.unwrap_or(f64::INFINITY),
                );
                return refuse("range", format!("{n} is not between {a} and {b}"));
            }
        }
        Kind::Select { options } => {
            if !options.iter().any(|o| v.as_str() == Some(o.value.as_str())) {
                return refuse("option", format!("{v} is not an option"));
            }
        }
        Kind::Text { max_length, .. } => {
            let max = max_length.unwrap_or(MAX_TEXT);
            if v.as_str().unwrap_or_default().chars().count() > max {
                return refuse("length", format!("longer than {max} characters"));
            }
        }
        Kind::Toggle | Kind::Color { .. } | Kind::Unsupported { .. } => {}
    }
    Ok(v.clone())
}

/// Values the application reported (`get`, `changed`): only known keys with a
/// value of the right type.
pub fn filter_values(schema: &Schema, values: &Value) -> Map<String, Value> {
    let Some(o) = values.as_object() else {
        return Map::new();
    };
    o.iter()
        .filter(|(k, v)| schema.field(k).is_some_and(|f| fits(&f.kind, v)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

// ---------------------------------------------------------------- Connection

/// Notification for the interface: `changed`, `schema-changed` or `closed`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveEvent {
    pub tool: String,
    pub method: String,
    pub params: Value,
}

pub type Notify = Arc<dyn Fn(LiveEvent) + Send + Sync>;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Status {
    pub running: bool,
    /// The application speaks the protocol of this tuxhub.
    pub protocol_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The application runs, but the connection failed (timeout, invalid
    /// answer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LiveError>,
}

/// Connections to the live interfaces of running applications, one per tool.
pub struct Live {
    runtime_dir: Option<PathBuf>,
    uid: u32,
    timeout: Duration,
    notify: Notify,
    slots: Mutex<HashMap<String, Slot>>,
}

fn not_running() -> LiveError {
    LiveError::new("not-running", "the application is not running")
}

fn invalid(m: impl Into<String>) -> LiveError {
    LiveError::new("invalid-answer", m)
}

fn closed() -> LiveError {
    LiveError::new("closed", "the application closed the connection")
}

/// What the application said in its answer to `hello`.
struct Hello {
    protocol_ok: bool,
    name: Option<String>,
    version: Option<String>,
    locale: String,
}

#[derive(Default)]
struct ConnState {
    pending: HashMap<u64, oneshot::Sender<Result<Value, LiveError>>>,
    /// Set once the connection is closed: why.
    closed: Option<LiveError>,
}

/// One connection to a running application.
struct Conn {
    tool: String,
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    state: Mutex<ConnState>,
    next_id: AtomicU64,
    hello: std::sync::OnceLock<Hello>,
    /// Checked schema, until the application reports `schema-changed`.
    schema: Mutex<Option<Arc<Schema>>>,
    closed_tx: watch::Sender<bool>,
    /// Closes the connection from the side of tuxhub.
    stop_tx: watch::Sender<bool>,
}

impl Conn {
    fn start(tool: &str, stream: UnixStream, notify: Notify) -> Arc<Self> {
        let (read, write) = stream.into_split();
        let (stop_tx, stop_rx) = watch::channel(false);
        let conn = Arc::new(Self {
            tool: tool.to_string(),
            writer: tokio::sync::Mutex::new(write),
            state: Default::default(),
            next_id: AtomicU64::new(0),
            hello: Default::default(),
            schema: Default::default(),
            closed_tx: watch::channel(false).0,
            stop_tx,
        });
        tokio::spawn(read_loop(conn.clone(), read, stop_rx, notify));
        conn
    }

    fn is_closed(&self) -> bool {
        self.state.lock().unwrap().closed.is_some()
    }

    fn protocol_ok(&self) -> bool {
        self.hello.get().is_some_and(|h| h.protocol_ok)
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, LiveError> {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.lock().unwrap();
            if let Some(reason) = &state.closed {
                return Err(reason.clone());
            }
            state.pending.insert(n, tx);
        }
        let mut line = json!({ "id": n, "method": method, "params": params }).to_string();
        line.push('\n');
        let answer = tokio::time::timeout(timeout, async {
            self.writer
                .lock()
                .await
                .write_all(line.as_bytes())
                .await
                .map_err(|_| closed())?;
            rx.await.map_err(|_| closed())?
        })
        .await;
        answer.unwrap_or_else(|_| {
            self.state.lock().unwrap().pending.remove(&n);
            Err(LiveError::new(
                "timeout",
                format!("the application did not answer {method:?} in time"),
            ))
        })
    }

    /// Fails the open requests with `reason` and closes the connection.
    async fn finish(&self, reason: LiveError) {
        let pending = {
            let mut state = self.state.lock().unwrap();
            state.closed = Some(reason.clone());
            std::mem::take(&mut state.pending)
        };
        for tx in pending.into_values() {
            let _ = tx.send(Err(reason.clone()));
        }
        self.closed_tx.send_replace(true);
        let _ = self.writer.lock().await.shutdown().await;
    }
}

/// Reads answers and notifications until the application closes the
/// connection, sends something invalid, or tuxhub closes it.
async fn read_loop(
    conn: Arc<Conn>,
    read: OwnedReadHalf,
    mut stop: watch::Receiver<bool>,
    notify: Notify,
) {
    let mut reader = BufReader::new(read);
    let mut buf = Vec::new();
    let reason = loop {
        buf.clear();
        // At most one byte more than allowed: enough to tell that a line is
        // too long, without reading it to the end.
        let mut limited = (&mut reader).take(MAX_LINE as u64 + 1);
        let read = tokio::select! {
            _ = async { let _ = stop.wait_for(|s| *s).await; } => break None,
            r = limited.read_until(b'\n', &mut buf) => r,
        };
        match read {
            Ok(0) | Err(_) => break Some(closed()),
            Ok(_) if buf.last() != Some(&b'\n') => {
                break Some(if buf.len() > MAX_LINE {
                    invalid("a line is longer than 1 MiB")
                } else {
                    closed()
                });
            }
            Ok(_) => buf.pop(),
        };
        let Ok(Value::Object(msg)) = serde_json::from_slice::<Value>(&buf) else {
            break Some(invalid("a line is not a JSON object"));
        };
        if let Some(id) = msg.get("id") {
            let Some(n) = id.as_u64() else {
                break Some(invalid("an answer has an invalid id"));
            };
            let reply = match msg.get("error") {
                Some(e) => Err(LiveError::new("rejected", error_text(e))),
                None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
            };
            // Answers that arrive after the timeout are dropped.
            if let Some(tx) = conn.state.lock().unwrap().pending.remove(&n) {
                let _ = tx.send(reply);
            }
            continue;
        }
        let event = |method: &str, params: Value| LiveEvent {
            tool: conn.tool.clone(),
            method: method.into(),
            params,
        };
        match msg.get("method").and_then(Value::as_str) {
            Some("changed") => {
                // Without a schema there is no page that shows values.
                let schema = conn.schema.lock().unwrap().clone();
                let values = msg.get("params").and_then(|p| p.get("values"));
                if let (Some(schema), Some(values)) = (schema, values) {
                    let values = filter_values(&schema, values);
                    if !values.is_empty() {
                        notify(event("changed", json!({ "values": values })));
                    }
                }
            }
            Some("schema-changed") => {
                *conn.schema.lock().unwrap() = None;
                notify(event("schema-changed", json!({})));
            }
            // Unknown notifications are left for newer versions of tuxhub.
            Some(_) => {}
            None => break Some(invalid("a line is neither an answer nor a notification")),
        }
    };
    match reason {
        Some(reason) => {
            conn.finish(reason.clone()).await;
            notify(LiveEvent {
                tool: conn.tool.clone(),
                method: "closed".into(),
                params: serde_json::to_value(reason).unwrap_or_default(),
            });
        }
        // Closed by tuxhub itself: nothing to report.
        None => conn.finish(closed()).await,
    }
}

/// The error text of the application, shortened.
fn error_text(e: &Value) -> String {
    e.get("message")
        .and_then(Value::as_str)
        .unwrap_or("the application refused the request")
        .chars()
        .take(MAX_ERROR)
        .collect()
}

fn parse_hello(v: &Value, locale: &str) -> Result<Hello, LiveError> {
    let o = v
        .as_object()
        .ok_or_else(|| invalid("hello: not an object"))?;
    let protocol = o
        .get("protocol")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("hello: `protocol` is not a number"))?;
    Ok(Hello {
        protocol_ok: protocol == PROTOCOL,
        name: text(o, "name", MAX_LABEL).map_err(|e| invalid(format!("hello: {e}")))?,
        version: text(o, "version", MAX_KEY).map_err(|e| invalid(format!("hello: {e}")))?,
        locale: locale.to_string(),
    })
}

/// User of this process: the owner of its `/proc` entry.
fn own_uid() -> u32 {
    std::fs::metadata("/proc/self").map_or(u32::MAX, |m| m.uid())
}

type Slot = Arc<tokio::sync::Mutex<Option<Arc<Conn>>>>;

impl Live {
    /// `runtime_dir` is `$XDG_RUNTIME_DIR`; without it no application is
    /// reachable.
    pub fn new(runtime_dir: Option<PathBuf>, notify: Notify) -> Self {
        Self {
            runtime_dir,
            uid: own_uid(),
            timeout: TIMEOUT,
            notify,
            slots: Default::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        runtime_dir: Option<PathBuf>,
        uid: u32,
        timeout: Duration,
        notify: Notify,
    ) -> Self {
        Self {
            runtime_dir,
            uid,
            timeout,
            notify,
            slots: Default::default(),
        }
    }

    /// Socket of tool `id`.
    pub fn socket(&self, id: &str) -> Option<PathBuf> {
        if !crate::manifest::valid_id(id) {
            return None;
        }
        let dir = self.runtime_dir.as_ref()?;
        Some(dir.join("tuxhub").join(format!("{id}.sock")))
    }

    fn slot(&self, id: &str) -> Slot {
        self.slots
            .lock()
            .unwrap()
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    /// Connects to a socket that passes the checks. `None` when nobody
    /// listens.
    async fn dial(&self, id: &str) -> Option<UnixStream> {
        let path = self.socket(id)?;
        check_socket(&path, self.uid).ok()?;
        tokio::time::timeout(self.timeout, UnixStream::connect(&path))
            .await
            .ok()?
            .ok()
    }

    /// The open connection to tool `id`, or a new one after `hello`.
    async fn conn(&self, id: &str, locale: Option<&str>) -> Result<Arc<Conn>, LiveError> {
        let slot = self.slot(id);
        let mut current = slot.lock().await;
        if let Some(c) = current.as_ref().filter(|c| !c.is_closed()) {
            return Ok(c.clone());
        }
        *current = None;
        let stream = self.dial(id).await.ok_or_else(not_running)?;
        let conn = Conn::start(id, stream, self.notify.clone());
        let locale = locale.unwrap_or("en");
        let params = json!({
            "protocol": PROTOCOL,
            "client": "tuxhub",
            "version": env!("CARGO_PKG_VERSION"),
            "locale": locale,
        });
        let hello = match conn.request("hello", params, self.timeout).await {
            Ok(v) => parse_hello(&v, locale),
            Err(e) => Err(e),
        };
        match hello {
            Ok(h) => {
                let _ = conn.hello.set(h);
                *current = Some(conn.clone());
                Ok(conn)
            }
            Err(e) => {
                conn.stop_tx.send_replace(true);
                Err(e)
            }
        }
    }

    /// A connection that speaks the protocol of this tuxhub.
    async fn speaking(&self, id: &str, locale: Option<&str>) -> Result<Arc<Conn>, LiveError> {
        let conn = self.conn(id, locale).await?;
        if !conn.protocol_ok() {
            return Err(LiveError::new(
                "protocol",
                format!("the application does not speak protocol {PROTOCOL}"),
            ));
        }
        Ok(conn)
    }

    pub async fn status(&self, id: &str, locale: &str) -> Status {
        match self.conn(id, Some(locale)).await {
            Ok(conn) => {
                let hello = conn.hello.get();
                Status {
                    running: true,
                    protocol_ok: conn.protocol_ok(),
                    name: hello.and_then(|h| h.name.clone()),
                    version: hello.and_then(|h| h.version.clone()),
                    error: None,
                }
            }
            // Gone while connecting.
            Err(e) if e.code == "not-running" || e.code == "closed" => Status::default(),
            Err(e) => Status {
                running: true,
                error: Some(e),
                ..Default::default()
            },
        }
    }

    async fn load_schema(&self, conn: &Conn, locale: &str) -> Result<Arc<Schema>, LiveError> {
        let v = conn
            .request("schema", json!({ "locale": locale }), self.timeout)
            .await?;
        let schema = Arc::new(parse_schema(&v)?);
        *conn.schema.lock().unwrap() = Some(schema.clone());
        Ok(schema)
    }

    /// The schema the values are checked against: the last one loaded, or a
    /// new one in the language of the connection.
    async fn current_schema(&self, conn: &Conn) -> Result<Arc<Schema>, LiveError> {
        let cached = conn.schema.lock().unwrap().clone();
        match cached {
            Some(s) => Ok(s),
            None => {
                let locale = conn.hello.get().map_or("en", |h| h.locale.as_str());
                self.load_schema(conn, locale).await
            }
        }
    }

    pub async fn schema(&self, id: &str, locale: &str) -> Result<Schema, LiveError> {
        let conn = self.speaking(id, Some(locale)).await?;
        Ok((*self.load_schema(&conn, locale).await?).clone())
    }

    pub async fn values(&self, id: &str) -> Result<Map<String, Value>, LiveError> {
        let conn = self.speaking(id, None).await?;
        let schema = self.current_schema(&conn).await?;
        let v = conn.request("get", json!({}), self.timeout).await?;
        let values = v
            .get("values")
            .filter(|v| v.is_object())
            .ok_or_else(|| invalid("get: `values` is missing"))?;
        Ok(filter_values(&schema, values))
    }

    /// Sends a value that passed the checks of the schema. Returns the value
    /// as the application applied it.
    pub async fn set(&self, id: &str, key: &str, value: &Value) -> Result<Value, LiveError> {
        let conn = self.speaking(id, None).await?;
        let schema = self.current_schema(&conn).await?;
        let field = schema
            .field(key)
            .ok_or_else(|| LiveError::new("unknown", format!("no setting {key:?}")))?;
        let value = check_set(field, value)?;
        let v = conn
            .request("set", json!({ "key": key, "value": value }), self.timeout)
            .await?;
        Ok(v.get("value")
            .filter(|v| fits(&field.kind, v))
            .cloned()
            .unwrap_or(value))
    }

    pub async fn action(&self, id: &str, action: &str) -> Result<Value, LiveError> {
        let conn = self.speaking(id, None).await?;
        let schema = self.current_schema(&conn).await?;
        if !schema.actions.iter().any(|a| a.id == action) {
            return Err(LiveError::new("unknown", format!("no action {action:?}")));
        }
        conn.request("action", json!({ "id": action }), self.timeout)
            .await
    }

    /// Asks the application to quit. `true` when it closed the connection and
    /// stopped listening within `wait`.
    pub async fn quit(&self, id: &str, wait: Duration) -> bool {
        let Ok(conn) = self.speaking(id, None).await else {
            return false;
        };
        let mut closed = conn.closed_tx.subscribe();
        tokio::time::timeout(wait, async {
            // The answer does not matter, only that the application ends.
            let _ = conn.request("quit", json!({}), wait).await;
            let _ = closed.wait_for(|c| *c).await;
            while self.dial(id).await.is_some() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Whether the application listens on its socket.
    pub async fn running(&self, id: &str) -> bool {
        let open = self
            .slot(id)
            .lock()
            .await
            .as_ref()
            .is_some_and(|c| !c.is_closed());
        open || self.dial(id).await.is_some()
    }
}

/// The socket must be a socket (not a symbolic link) of user `uid`.
pub fn check_socket(path: &Path, uid: u32) -> Result<(), String> {
    let md = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !md.file_type().is_socket() {
        return Err(format!("{} is not a socket", path.display()));
    }
    if md.uid() != uid {
        return Err(format!("{} belongs to another user", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(fields: Value) -> Schema {
        parse_schema(&json!({
            "sections": [{ "id": "main", "label": "Main", "fields": fields }],
            "actions": []
        }))
        .unwrap()
    }

    fn all_types() -> Schema {
        schema(json!([
            { "key": "on", "type": "toggle", "label": "On" },
            { "key": "size", "type": "number", "label": "Size", "min": 8, "max": 120, "step": 1, "unit": "px", "slider": true, "default": 26 },
            { "key": "font", "type": "select", "label": "Font",
              "options": [{ "value": "sans", "label": "Sans" }, { "value": "mono", "label": "Mono" }] },
            { "key": "name", "type": "text", "label": "Name", "max_length": 5 },
            { "key": "color", "type": "color", "label": "Color" },
            { "key": "shade", "type": "color", "label": "Shade", "alpha": true },
            { "key": "off", "type": "toggle", "label": "Off", "disabled": true }
        ]))
    }

    fn field<'a>(s: &'a Schema, key: &str) -> &'a Field {
        s.field(key).unwrap()
    }

    #[test]
    fn reads_every_field_type() {
        let s = all_types();
        assert!(s.invalid.is_empty(), "{:?}", s.invalid);
        assert_eq!(s.sections[0].fields.len(), 7);
        assert_eq!(
            field(&s, "size").kind,
            Kind::Number {
                min: Some(8.0),
                max: Some(120.0),
                step: Some(1.0),
                unit: Some("px".into()),
                slider: true
            }
        );
        assert_eq!(field(&s, "size").default, Some(json!(26)));
        assert_eq!(field(&s, "shade").kind, Kind::Color { alpha: true });
        assert!(field(&s, "off").disabled);
        // The interface tells the types apart by `type`.
        let out = serde_json::to_value(field(&s, "font")).unwrap();
        assert_eq!(out["type"], "select");
        assert_eq!(out["options"][1]["label"], "Mono");
    }

    /// 7.7: a type this tuxhub does not know becomes a notice; the other
    /// fields stay normal.
    #[test]
    fn unknown_type_is_kept_as_unsupported() {
        let s = schema(json!([
            { "key": "when", "type": "date", "label": "Date" },
            { "key": "on", "type": "toggle", "label": "On" }
        ]));
        assert!(s.invalid.is_empty());
        assert_eq!(
            field(&s, "when").kind,
            Kind::Unsupported {
                declared: "date".into()
            }
        );
        assert_eq!(field(&s, "on").kind, Kind::Toggle);
        let out = serde_json::to_value(field(&s, "when")).unwrap();
        assert_eq!(out["type"], "unsupported");
        assert!(check_set(field(&s, "when"), &json!("2026-10-25")).is_err());
    }

    /// 7.8: the second field with the same key is left out, also across
    /// sections.
    #[test]
    fn duplicate_key_drops_the_second_field() {
        let s = parse_schema(&json!({
            "sections": [
                { "id": "a", "label": "A", "fields": [
                    { "key": "size", "type": "number", "label": "First" },
                    { "key": "size", "type": "toggle", "label": "Second" }
                ]},
                { "id": "b", "label": "B", "fields": [
                    { "key": "size", "type": "text", "label": "Third" }
                ]}
            ]
        }))
        .unwrap();
        assert_eq!(field(&s, "size").label, "First");
        assert_eq!(s.sections[0].fields.len(), 1);
        assert!(s.sections[1].fields.is_empty());
        assert_eq!(s.invalid.len(), 2);
        assert!(s.invalid.iter().all(|i| i.key == "size"));
    }

    #[test]
    fn invalid_fields_are_listed_and_the_rest_stays() {
        let long = "x".repeat(121);
        let s = schema(json!([
            { "key": "bad key", "type": "toggle", "label": "Space" },
            { "key": "nolabel", "type": "toggle" },
            { "key": "long", "type": "toggle", "label": long },
            { "key": "empty", "type": "select", "label": "No options", "options": [] },
            { "key": "range", "type": "number", "label": "Range", "min": 10, "max": 1 },
            { "key": "notype", "label": "No type" },
            "not an object",
            { "key": "ok", "type": "toggle", "label": "Fine" }
        ]));
        let keys: Vec<&str> = s.invalid.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "bad key", "nolabel", "long", "empty", "range", "notype", "#6"
            ]
        );
        assert!(s.invalid.iter().all(|i| !i.reason.is_empty()));
        assert_eq!(s.sections[0].fields.len(), 1);
        assert_eq!(s.sections[0].fields[0].key, "ok");
    }

    #[test]
    fn rejects_a_schema_without_sections() {
        assert!(parse_schema(&json!("nope")).is_err());
        assert!(parse_schema(&json!({ "sections": 3 })).is_err());
        // Actions alone are fine.
        let s = parse_schema(&json!({ "actions": [{ "id": "go", "label": "Go" }] })).unwrap();
        assert_eq!(s.actions.len(), 1);
    }

    #[test]
    fn checks_actions() {
        let s = parse_schema(&json!({
            "sections": [],
            "actions": [
                { "id": "reset", "label": "Reset", "danger": true, "confirm": "Sure?" },
                { "id": "reset", "label": "Again" },
                { "id": "", "label": "Empty" },
                { "id": "open" }
            ]
        }))
        .unwrap();
        assert_eq!(
            s.actions,
            [Action {
                id: "reset".into(),
                label: "Reset".into(),
                danger: true,
                confirm: Some("Sure?".into())
            }]
        );
        assert_eq!(s.invalid.len(), 3);
    }

    #[test]
    fn limits_the_number_of_fields() {
        let fields: Vec<Value> = (0..MAX_FIELDS + 5)
            .map(|i| json!({ "key": format!("k{i}"), "type": "toggle", "label": "T" }))
            .collect();
        let s = schema(Value::Array(fields));
        assert_eq!(s.sections[0].fields.len(), MAX_FIELDS);
        assert_eq!(s.invalid.len(), 1);
    }

    /// A default that does not fit the field is ignored, the field stays.
    #[test]
    fn ignores_a_default_of_the_wrong_type() {
        let s = schema(json!([{ "key": "on", "type": "toggle", "label": "On", "default": "yes" }]));
        assert!(s.invalid.is_empty());
        assert_eq!(field(&s, "on").default, None);
    }

    /// 7.14: labels are text; tuxhub keeps them as they are and the interface
    /// shows them as text.
    #[test]
    fn labels_with_html_stay_text() {
        let s = schema(json!([
            { "key": "on", "type": "toggle", "label": "<b>Bold</b>", "help": "<img src=x onerror=alert(1)>" }
        ]));
        assert_eq!(field(&s, "on").label, "<b>Bold</b>");
        assert_eq!(
            field(&s, "on").help.as_deref(),
            Some("<img src=x onerror=alert(1)>")
        );
    }

    /// 7.9: values that do not fit the field are refused with a reason.
    #[test]
    fn refuses_values_that_do_not_fit() {
        let s = all_types();
        let code = |key: &str, v: Value| check_set(field(&s, key), &v).unwrap_err().code;
        assert_eq!(code("size", json!(121)), "range");
        assert_eq!(code("size", json!(7.5)), "range");
        assert_eq!(code("size", json!("26")), "type");
        assert_eq!(code("font", json!("serif")), "option");
        assert_eq!(code("color", json!("red")), "color");
        assert_eq!(code("color", json!("#12345")), "color");
        assert_eq!(code("color", json!("#11223344")), "color");
        assert_eq!(code("shade", json!("#1122334")), "color");
        assert_eq!(code("name", json!("toolong")), "length");
        assert_eq!(code("on", json!(1)), "type");
        assert_eq!(code("off", json!(true)), "disabled");

        let ok = |key: &str, v: Value| check_set(field(&s, key), &v).unwrap();
        assert_eq!(ok("size", json!(120)), json!(120));
        assert_eq!(ok("size", json!(8)), json!(8));
        assert_eq!(ok("font", json!("mono")), json!("mono"));
        assert_eq!(ok("color", json!("#a0B1c2")), json!("#a0B1c2"));
        assert_eq!(ok("shade", json!("#a0b1c2ff")), json!("#a0b1c2ff"));
        assert_eq!(ok("shade", json!("#a0b1c2")), json!("#a0b1c2"));
        // Length counts characters, not bytes.
        assert_eq!(ok("name", json!("äöüß€")), json!("äöüß€"));
        assert_eq!(ok("on", json!(false)), json!(false));
    }

    #[test]
    fn keeps_only_known_values_of_the_right_type() {
        let s = all_types();
        let v = filter_values(
            &s,
            &json!({
                "on": true,
                "size": 300,
                "font": "anything",
                "name": 5,
                "color": "blue",
                "shade": "#00000080",
                "unknown": 1
            }),
        );
        // The application decides about range and options of its own values.
        assert_eq!(
            Value::Object(v),
            json!({ "on": true, "size": 300, "font": "anything", "shade": "#00000080" })
        );
        assert!(filter_values(&s, &json!([1, 2])).is_empty());
    }
}

/// Edge cases of the connection (SPEC 7), against a test application that
/// listens on a Unix socket.
#[cfg(test)]
mod conn_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::net::UnixListener;
    use tokio::sync::broadcast;

    /// What the test application does with a request.
    enum Do {
        /// Answer with `result`.
        Result(Value),
        /// Answer with this error object.
        Error(Value),
        /// Send these bytes instead of an answer.
        Raw(Vec<u8>),
        /// Never answer.
        Silent,
        /// Answer with `true` and close the connection; the application keeps
        /// running.
        CloseAfter,
        /// Exit without answering: listener and every connection close.
        Exit,
        /// Answer with `true`, then exit.
        AnswerAndExit,
    }

    type Handler = Arc<dyn Fn(&str, &Value) -> Do + Send + Sync>;
    type Received = Arc<Mutex<Vec<(String, Value)>>>;
    type Events = Arc<Mutex<Vec<LiveEvent>>>;

    fn test_schema() -> Value {
        json!({
            "sections": [{ "id": "main", "label": "Main", "fields": [
                { "key": "size", "type": "number", "label": "Size", "min": 8, "max": 120 },
                { "key": "on", "type": "toggle", "label": "On" }
            ]}],
            "actions": [{ "id": "reset", "label": "Reset" }]
        })
    }

    /// The usual answers of a well-behaved application.
    fn standard(method: &str, params: &Value) -> Do {
        match method {
            "hello" => Do::Result(json!({ "protocol": 1, "name": "Test", "version": "1.0" })),
            "schema" => Do::Result(test_schema()),
            "get" => Do::Result(json!({ "values": { "size": 26, "on": true, "secret": 1 } })),
            "set" => Do::Result(json!({ "value": params["value"] })),
            "action" => Do::Result(json!(true)),
            "quit" => Do::AnswerAndExit,
            _ => Do::Error(json!({ "message": "unknown method" })),
        }
    }

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TestApp {
        /// `$XDG_RUNTIME_DIR` of the test
        dir: PathBuf,
        received: Received,
        broadcast: broadcast::Sender<String>,
        exit: watch::Sender<bool>,
    }

    impl TestApp {
        async fn start(handler: impl Fn(&str, &Value) -> Do + Send + Sync + 'static) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "tuxhub-live-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("tuxhub")).unwrap();
            let listener = UnixListener::bind(dir.join("tuxhub/demo.sock")).unwrap();
            let handler: Handler = Arc::new(handler);
            let received: Received = Default::default();
            let (broadcast, _) = broadcast::channel(16);
            let (exit, _) = watch::channel(false);
            let app = Self {
                dir,
                received: received.clone(),
                broadcast: broadcast.clone(),
                exit: exit.clone(),
            };
            let mut exit_rx = exit.subscribe();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = async { let _ = exit_rx.wait_for(|e| *e).await; } => break,
                        Ok((stream, _)) = listener.accept() => {
                            tokio::spawn(serve(
                                stream,
                                handler.clone(),
                                received.clone(),
                                broadcast.subscribe(),
                                exit.clone(),
                            ));
                        }
                    }
                }
                // The listener is dropped here; the socket file stays, as
                // after a crash.
            });
            app
        }

        fn live(&self) -> (Live, Events) {
            self.live_as(own_uid())
        }

        fn live_as(&self, uid: u32) -> (Live, Events) {
            let events: Events = Default::default();
            let sink = events.clone();
            let live = Live::for_test(
                Some(self.dir.clone()),
                uid,
                Duration::from_millis(300),
                Arc::new(move |e| sink.lock().unwrap().push(e)),
            );
            (live, events)
        }

        /// Sends a notification to every connection.
        fn notify(&self, msg: Value) {
            self.broadcast.send(msg.to_string()).unwrap();
        }

        fn methods(&self) -> Vec<String> {
            self.received
                .lock()
                .unwrap()
                .iter()
                .map(|(m, _)| m.clone())
                .collect()
        }
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            let _ = self.exit.send(true);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn serve(
        stream: UnixStream,
        handler: Handler,
        received: Received,
        mut notes: broadcast::Receiver<String>,
        exit: watch::Sender<bool>,
    ) {
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let mut exit_rx = exit.subscribe();
        loop {
            tokio::select! {
                _ = async { let _ = exit_rx.wait_for(|e| *e).await; } => return,
                Ok(note) = notes.recv() => {
                    write.write_all(format!("{note}\n").as_bytes()).await.unwrap();
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { return };
                    let msg: Value = serde_json::from_str(&line).unwrap();
                    let method = msg["method"].as_str().unwrap().to_string();
                    received.lock().unwrap().push((method.clone(), msg["params"].clone()));
                    let id = msg["id"].clone();
                    let answer = |m: Value| format!("{m}\n").into_bytes();
                    let out = match handler(&method, &msg["params"]) {
                        Do::Result(r) => answer(json!({ "id": id, "result": r })),
                        Do::Error(e) => answer(json!({ "id": id, "error": e })),
                        Do::Raw(bytes) => bytes,
                        Do::Silent => continue,
                        Do::CloseAfter => {
                            let _ = write.write_all(&answer(json!({ "id": id, "result": true }))).await;
                            return;
                        }
                        Do::Exit => {
                            let _ = exit.send(true);
                            return;
                        }
                        Do::AnswerAndExit => {
                            let _ = write.write_all(&answer(json!({ "id": id, "result": true }))).await;
                            let _ = exit.send(true);
                            return;
                        }
                    };
                    if write.write_all(&out).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn own_uid() -> u32 {
        std::fs::metadata("/proc/self").unwrap().uid()
    }

    async fn wait_until(what: &str, cond: impl Fn() -> bool) {
        for _ in 0..100 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {what}");
    }

    fn running(name: &str) -> Status {
        Status {
            running: true,
            protocol_ok: true,
            name: Some(name.into()),
            version: Some("1.0".into()),
            error: None,
        }
    }

    #[tokio::test]
    async fn talks_to_a_running_application() {
        let app = TestApp::start(standard).await;
        let (live, _) = app.live();
        assert_eq!(live.status("demo", "de").await, running("Test"));
        assert!(live.running("demo").await);
        let schema = live.schema("demo", "de").await.unwrap();
        assert_eq!(schema.sections[0].fields.len(), 2);
        // Only known keys come back.
        assert_eq!(
            Value::Object(live.values("demo").await.unwrap()),
            json!({ "size": 26, "on": true })
        );
        assert_eq!(
            live.set("demo", "size", &json!(40)).await.unwrap(),
            json!(40)
        );
        assert_eq!(live.action("demo", "reset").await.unwrap(), json!(true));
        // One connection for all requests, `hello` first, with the locale.
        assert_eq!(app.methods(), ["hello", "schema", "get", "set", "action"]);
        let received = app.received.lock().unwrap().clone();
        assert_eq!(received[0].1["protocol"], 1);
        assert_eq!(received[0].1["client"], "tuxhub");
        assert_eq!(received[0].1["locale"], "de");
        assert_eq!(received[1].1, json!({ "locale": "de" }));
        assert_eq!(received[3].1, json!({ "key": "size", "value": 40 }));
    }

    #[tokio::test]
    async fn refuses_unknown_keys_and_actions_without_sending() {
        let app = TestApp::start(standard).await;
        let (live, _) = app.live();
        let err = live.set("demo", "secret", &json!(1)).await.unwrap_err();
        assert_eq!(err.code, "unknown");
        let err = live.action("demo", "format-disk").await.unwrap_err();
        assert_eq!(err.code, "unknown");
        assert_eq!(app.methods(), ["hello", "schema"]);
    }

    /// 7.1: no socket: not running, and no error.
    #[tokio::test]
    async fn no_socket_means_not_running() {
        let app = TestApp::start(standard).await;
        std::fs::remove_file(app.dir.join("tuxhub/demo.sock")).unwrap();
        let (live, events) = app.live();
        assert_eq!(live.status("demo", "en").await, Status::default());
        assert!(!live.running("demo").await);
        assert_eq!(
            live.schema("demo", "en").await.unwrap_err().code,
            "not-running"
        );
        assert!(events.lock().unwrap().is_empty());

        // Without $XDG_RUNTIME_DIR nothing is reachable.
        let live = Live::for_test(None, own_uid(), TIMEOUT, Arc::new(|_| {}));
        assert_eq!(live.socket("demo"), None);
        assert_eq!(live.status("demo", "en").await, Status::default());
    }

    /// 7.2: a socket file nobody listens on: not running.
    #[tokio::test]
    async fn stale_socket_means_not_running() {
        let app = TestApp::start(standard).await;
        let _ = app.exit.send(true);
        let path = app.dir.join("tuxhub/demo.sock");
        wait_until("listener closed", || {
            std::os::unix::net::UnixStream::connect(&path).is_err()
        })
        .await;
        assert!(path.exists());
        let (live, _) = app.live();
        assert_eq!(live.status("demo", "en").await, Status::default());
        assert!(!live.running("demo").await);
    }

    /// 7.3: a socket of another user is not used, neither is a symbolic link
    /// or a regular file.
    #[tokio::test]
    async fn foreign_sockets_are_not_used() {
        let app = TestApp::start(standard).await;
        let path = app.dir.join("tuxhub/demo.sock");
        assert_eq!(check_socket(&path, own_uid()), Ok(()));
        assert!(check_socket(&path, own_uid() + 1).is_err());
        let (live, _) = app.live_as(own_uid() + 1);
        assert_eq!(live.status("demo", "en").await, Status::default());
        assert!(!live.running("demo").await);
        assert!(app.methods().is_empty());

        let link = app.dir.join("tuxhub/link.sock");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(check_socket(&link, own_uid()).is_err());
        let (live, _) = app.live();
        assert!(!live.running("link").await);
        let file = app.dir.join("tuxhub/file.sock");
        std::fs::write(&file, "").unwrap();
        assert!(check_socket(&file, own_uid()).is_err());
        assert!(check_socket(&app.dir.join("tuxhub/none.sock"), own_uid()).is_err());
    }

    /// 7.4: an application that does not answer in time: the request fails
    /// with a timeout, and the connection keeps working.
    #[tokio::test]
    async fn timeout_keeps_the_connection_usable() {
        let app = TestApp::start(|m, p| match m {
            "set" => Do::Silent,
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let started = std::time::Instant::now();
        let err = live.set("demo", "size", &json!(40)).await.unwrap_err();
        assert_eq!(err.code, "timeout");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(live.values("demo").await.unwrap()["size"], json!(26));
        assert!(live.status("demo", "en").await.running);
    }

    /// The application does not answer `hello`: it runs, but with an error.
    #[tokio::test]
    async fn silent_hello_is_a_timeout() {
        let app = TestApp::start(|_, _| Do::Silent).await;
        let (live, _) = app.live();
        let status = live.status("demo", "en").await;
        assert!(status.running);
        assert!(!status.protocol_ok);
        assert_eq!(status.error.unwrap().code, "timeout");
    }

    /// 7.5: no JSON: the connection is closed, the request fails with
    /// "invalid answer", the interface is told.
    #[tokio::test]
    async fn invalid_json_closes_the_connection() {
        let app = TestApp::start(|m, p| match m {
            "get" => Do::Raw(b"this is not json\n".to_vec()),
            _ => standard(m, p),
        })
        .await;
        let (live, events) = app.live();
        let err = live.values("demo").await.unwrap_err();
        assert_eq!(err.code, "invalid-answer");
        wait_until("closed event", || !events.lock().unwrap().is_empty()).await;
        let ev = events.lock().unwrap()[0].clone();
        assert_eq!(ev.tool, "demo");
        assert_eq!(ev.method, "closed");
        assert_eq!(ev.params["code"], "invalid-answer");
        // A new request opens a new connection.
        live.schema("demo", "en").await.unwrap();
        assert_eq!(app.methods(), ["hello", "schema", "get", "hello", "schema"]);
    }

    /// 7.5: a line over 1 MiB is not read to the end.
    #[tokio::test]
    async fn overlong_line_closes_the_connection() {
        let app = TestApp::start(|m, p| match m {
            "get" => {
                let mut line = format!("{{\"id\":3,\"result\":\"{}", "a".repeat(MAX_LINE));
                line.push_str("\"}\n");
                Do::Raw(line.into_bytes())
            }
            _ => standard(m, p),
        })
        .await;
        let (live, events) = app.live();
        assert_eq!(
            live.values("demo").await.unwrap_err().code,
            "invalid-answer"
        );
        wait_until("closed event", || !events.lock().unwrap().is_empty()).await;
    }

    /// A line of exactly 1 MiB is still fine.
    #[tokio::test]
    async fn line_of_one_mib_is_read() {
        let pad = MAX_LINE - r#"{"id":2,"result":{"sections":[],"x":""}}"#.len();
        let app = TestApp::start(move |m, p| match m {
            "schema" => Do::Result(json!({ "sections": [], "x": "b".repeat(pad) })),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        live.schema("demo", "en").await.unwrap();
    }

    /// 7.6: another protocol version: no fields, no further requests.
    #[tokio::test]
    async fn other_protocol_shows_no_fields() {
        let app = TestApp::start(|m, p| match m {
            "hello" => Do::Result(json!({ "protocol": 2, "name": "Future", "version": "9" })),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let status = live.status("demo", "en").await;
        assert!(status.running);
        assert!(!status.protocol_ok);
        assert_eq!(status.name.as_deref(), Some("Future"));
        assert_eq!(
            live.schema("demo", "en").await.unwrap_err().code,
            "protocol"
        );
        assert_eq!(live.values("demo").await.unwrap_err().code, "protocol");
        assert_eq!(app.methods(), ["hello"]);
    }

    /// An invalid `hello` answer is an invalid answer.
    #[tokio::test]
    async fn invalid_hello_is_reported() {
        let app = TestApp::start(|m, p| match m {
            "hello" => Do::Result(json!({ "protocol": "one" })),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let status = live.status("demo", "en").await;
        assert!(status.running);
        assert_eq!(status.error.unwrap().code, "invalid-answer");
    }

    /// 7.9: values that do not fit are refused before anything is sent.
    #[tokio::test]
    async fn invalid_values_are_not_sent() {
        let app = TestApp::start(standard).await;
        let (live, _) = app.live();
        assert_eq!(
            live.set("demo", "size", &json!(121))
                .await
                .unwrap_err()
                .code,
            "range"
        );
        assert_eq!(
            live.set("demo", "on", &json!("yes"))
                .await
                .unwrap_err()
                .code,
            "type"
        );
        assert!(!app.methods().contains(&"set".to_string()));
    }

    /// 7.10: the application refuses a value: its text comes back.
    #[tokio::test]
    async fn refused_value_returns_the_application_text() {
        let app = TestApp::start(|m, p| match m {
            "set" => Do::Error(json!({ "message": "Too big for this screen", "field": "size" })),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let err = live.set("demo", "size", &json!(100)).await.unwrap_err();
        assert_eq!(err, LiveError::new("rejected", "Too big for this screen"));
        // An overlong text is shortened.
        let app = TestApp::start(|m, p| match m {
            "set" => Do::Error(json!({ "message": "x".repeat(10_000) })),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let err = live.set("demo", "size", &json!(100)).await.unwrap_err();
        assert_eq!(err.message.chars().count(), MAX_ERROR);
    }

    /// 7.11: the application exits during a request: the request fails at
    /// once, and the application is no longer running.
    #[tokio::test]
    async fn exit_during_a_request_fails_it_cleanly() {
        let app = TestApp::start(|m, p| match m {
            "action" => Do::Exit,
            _ => standard(m, p),
        })
        .await;
        let (live, events) = app.live();
        live.schema("demo", "en").await.unwrap();
        let started = std::time::Instant::now();
        let err = live.action("demo", "reset").await.unwrap_err();
        assert_eq!(err.code, "closed");
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "{:?}",
            started.elapsed()
        );
        wait_until("closed event", || {
            events.lock().unwrap().iter().any(|e| e.method == "closed")
        })
        .await;
        assert_eq!(live.status("demo", "en").await, Status::default());
    }

    /// `changed` is passed on with the known keys only.
    #[tokio::test]
    async fn changed_values_are_passed_on() {
        let app = TestApp::start(standard).await;
        let (live, events) = app.live();
        live.schema("demo", "en").await.unwrap();
        app.notify(json!({ "method": "changed", "params": { "values": { "size": 50, "x": 1, "on": "no" } } }));
        app.notify(json!({ "method": "unknown-note", "params": {} }));
        wait_until("changed event", || !events.lock().unwrap().is_empty()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let evs = events.lock().unwrap().clone();
        assert_eq!(
            evs,
            [LiveEvent {
                tool: "demo".into(),
                method: "changed".into(),
                params: json!({ "values": { "size": 50 } }),
            }]
        );
    }

    /// 7.13: `schema-changed` is passed on, and the next request loads the
    /// new schema.
    #[tokio::test]
    async fn schema_changed_loads_the_schema_again() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let app = TestApp::start(move |m, p| match m {
            "schema" if c.fetch_add(1, Ordering::Relaxed) > 0 => Do::Result(json!({
                "sections": [{ "id": "main", "label": "Main", "fields": [
                    { "key": "size", "type": "number", "label": "Size", "min": 8, "max": 300 }
                ]}]
            })),
            _ => standard(m, p),
        })
        .await;
        let (live, events) = app.live();
        live.schema("demo", "en").await.unwrap();
        assert_eq!(
            live.set("demo", "size", &json!(200))
                .await
                .unwrap_err()
                .code,
            "range"
        );
        app.notify(json!({ "method": "schema-changed", "params": {} }));
        wait_until("schema-changed event", || {
            !events.lock().unwrap().is_empty()
        })
        .await;
        assert_eq!(events.lock().unwrap()[0].method, "schema-changed");
        // The value is checked against the new schema.
        assert_eq!(
            live.set("demo", "size", &json!(200)).await.unwrap(),
            json!(200)
        );
        assert_eq!(live.values("demo").await.unwrap()["size"], json!(26));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    /// 7.15: `quit` closes the application; one that ignores it is reported.
    #[tokio::test]
    async fn quit_waits_for_the_application() {
        let app = TestApp::start(standard).await;
        let (live, _) = app.live();
        assert!(live.quit("demo", Duration::from_secs(3)).await);
        assert_eq!(app.methods(), ["hello", "quit"]);
        assert!(!live.running("demo").await);

        // Answers, but keeps running and connected.
        let app = TestApp::start(|m, p| match m {
            "quit" => Do::Result(json!(true)),
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        let started = std::time::Instant::now();
        assert!(!live.quit("demo", Duration::from_millis(400)).await);
        assert!(started.elapsed() >= Duration::from_millis(400));

        // Closes only the connection and keeps running.
        let app = TestApp::start(|m, p| match m {
            "quit" => Do::CloseAfter,
            _ => standard(m, p),
        })
        .await;
        let (live, _) = app.live();
        assert!(!live.quit("demo", Duration::from_millis(400)).await);

        // Not running: nothing to quit.
        let app = TestApp::start(standard).await;
        std::fs::remove_file(app.dir.join("tuxhub/demo.sock")).unwrap();
        let (live, _) = app.live();
        assert!(!live.quit("demo", Duration::from_millis(400)).await);
    }

    /// 7.16: two connections (two tuxhub windows or instances) both receive
    /// `changed`.
    #[tokio::test]
    async fn two_connections_both_receive_changes() {
        let app = TestApp::start(standard).await;
        let (a, events_a) = app.live();
        let (b, events_b) = app.live();
        a.schema("demo", "en").await.unwrap();
        b.schema("demo", "de").await.unwrap();
        app.notify(json!({ "method": "changed", "params": { "values": { "on": false } } }));
        wait_until("both events", || {
            !events_a.lock().unwrap().is_empty() && !events_b.lock().unwrap().is_empty()
        })
        .await;
        for events in [events_a, events_b] {
            assert_eq!(
                events.lock().unwrap()[0].params,
                json!({ "values": { "on": false } })
            );
        }
    }
}
