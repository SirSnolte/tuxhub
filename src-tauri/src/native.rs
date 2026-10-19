//! Laufzeit für native Add-ons.
//!
//! Jedes Add-on läuft als eigener Prozess. Der Hub schreibt JSON-RPC-Anfragen
//! zeilenweise auf stdin (`{"id":1,"method":"…","params":…}`), das Add-on
//! antwortet auf stdout (`{"id":1,"result":…}` bzw. `{"id":1,"error":{"message":…}}`)
//! und kann jederzeit Benachrichtigungen ohne `id` senden
//! (`{"method":"status","params":{…}}`). Die landen als Tauri-Event
//! `addon-event` im Frontend.

use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};

type Reply = Result<Value, String>;
type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Reply>>>>;

struct Proc {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
}

#[derive(Clone, Serialize)]
pub struct AddonEvent {
    pub addon: String,
    pub method: String,
    pub params: Value,
}

#[derive(Default)]
pub struct Runtime {
    procs: Mutex<HashMap<String, Proc>>,
    next_id: AtomicU64,
}

impl Runtime {
    /// Startet das Add-on, falls es nicht schon läuft. Gibt `true` zurück, wenn
    /// es neu gestartet wurde.
    pub async fn ensure(
        &self,
        app: &AppHandle,
        id: &str,
        exe: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<bool, String> {
        let mut procs = self.procs.lock().await;
        if let Some(p) = procs.get_mut(id) {
            if matches!(p.child.try_wait(), Ok(None)) {
                return Ok(false);
            }
            procs.remove(id);
        }

        let mut child = Command::new(exe)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("{} konnte nicht gestartet werden: {e}", exe.display()))?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let pending: Pending = Default::default();

        let pending2 = pending.clone();
        let app2 = app.clone();
        let addon = id.to_string();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    eprintln!("[{addon}] {line}");
                    continue;
                };
                if let Some(n) = msg.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&n) {
                        let reply = match msg.get("error") {
                            Some(e) => Err(e
                                .get("message")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .unwrap_or_else(|| e.to_string())),
                            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(reply);
                    }
                } else if let Some(method) = msg.get("method").and_then(Value::as_str) {
                    if method == "shortcuts" {
                        crate::tool_shortcuts_changed(
                            &app2,
                            &addon,
                            msg.get("params").unwrap_or(&Value::Null),
                        );
                    }
                    let _ = app2.emit(
                        "addon-event",
                        AddonEvent {
                            addon: addon.clone(),
                            method: method.into(),
                            params: msg.get("params").cloned().unwrap_or(Value::Null),
                        },
                    );
                }
            }
            // Offene Anfragen scheitern, sobald die Sender fallen gelassen werden.
            pending2.lock().unwrap().clear();
            let _ = app2.emit(
                "addon-event",
                AddonEvent {
                    addon,
                    method: "exited".into(),
                    params: Value::Null,
                },
            );
        });

        procs.insert(
            id.to_string(),
            Proc {
                child,
                stdin,
                pending,
            },
        );
        Ok(true)
    }

    pub async fn call(&self, id: &str, method: &str, params: Value) -> Reply {
        let n = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        {
            let mut procs = self.procs.lock().await;
            let p = procs.get_mut(id).ok_or("Add-on läuft nicht")?;
            p.pending.lock().unwrap().insert(n, tx);
            let mut line = json!({ "id": n, "method": method, "params": params }).to_string();
            line.push('\n');
            p.stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Senden an Add-on fehlgeschlagen: {e}"))?;
            p.stdin.flush().await.map_err(|e| e.to_string())?;
        }
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| "Add-on antwortet nicht".to_string())?
            .map_err(|_| "Add-on wurde beendet".to_string())?
    }

    pub async fn is_running(&self, id: &str) -> bool {
        let mut procs = self.procs.lock().await;
        procs
            .get_mut(id)
            .is_some_and(|p| matches!(p.child.try_wait(), Ok(None)))
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self.procs.lock().await.keys().cloned().collect();
        for id in ids {
            self.stop(&id).await;
        }
    }

    pub async fn stop(&self, id: &str) {
        let proc = self.procs.lock().await.remove(id);
        if let Some(mut p) = proc {
            // Erst höflich fragen, dann hart beenden.
            let _ = p.stdin.write_all(b"{\"method\":\"shutdown\"}\n").await;
            let _ = p.stdin.flush().await;
            if tokio::time::timeout(Duration::from_secs(2), p.child.wait())
                .await
                .is_err()
            {
                let _ = p.child.kill().await;
            }
        }
    }
}
