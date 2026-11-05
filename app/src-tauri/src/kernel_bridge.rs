//! Hosts the kernel inside the shell process and frames its replies.
//!
//! Wire format of `kernel_call`: a four byte ASCII tag followed by the
//! payload. The tag is `JSON` when the payload is the UTF-8 JSON reply
//! envelope and `BLOB` when it is a raw buffer with its own header.
//!
//! `kernel_call` runs off the main thread, so the window keeps drawing and
//! answering while a long command runs; `kernel_progress` says how far it
//! has got and `kernel_cancel` asks it to stop, both without waiting for
//! the kernel.

use std::sync::Mutex;

use carve_core::{Kernel, KernelReply};
use tauri::ipc::Response;
use tauri::State;

/// Length of the reply tag in bytes.
pub const TAG_LEN: usize = 4;
/// Tag of a UTF-8 JSON payload.
pub const TAG_JSON: &[u8; TAG_LEN] = b"JSON";
/// Tag of a raw binary payload.
pub const TAG_BLOB: &[u8; TAG_LEN] = b"BLOB";

/// Managed state: the one open kernel. Commands run one at a time.
pub struct KernelHost {
    kernel: Mutex<Kernel>,
}

impl KernelHost {
    /// Creates a host holding a kernel with an empty document.
    pub fn new() -> Self {
        KernelHost {
            kernel: Mutex::new(Kernel::new()),
        }
    }

    /// Runs a command and returns the framed reply. Kernel errors travel
    /// inside the reply envelope, so only host failures land in `Err`.
    fn call(&self, name: &str, args: &str) -> Result<Vec<u8>, String> {
        let mut kernel = self
            .kernel
            .lock()
            .map_err(|_| "kernel lock was poisoned".to_string())?;
        Ok(frame(kernel.call(name, args)))
    }
}

impl Default for KernelHost {
    fn default() -> Self {
        KernelHost::new()
    }
}

/// Prefixes a reply with its tag.
fn frame(reply: KernelReply) -> Vec<u8> {
    reply.into_framed()
}

/// Runs one kernel command. `args` is a JSON string. Runs on a worker
/// thread, not the main one, so the window is never held up by it.
#[tauri::command(async)]
pub fn kernel_call(
    name: String,
    args: Option<String>,
    host: State<'_, KernelHost>,
) -> Result<Response, String> {
    let args = args.unwrap_or_default();
    let framed = host.call(&name, &args)?;
    #[cfg(debug_assertions)]
    eprintln!("kernel_call {name} -> {} bytes", framed.len());
    Ok(Response::new(framed))
}

/// Asks the running command to stop (there is only ever one; `id` is kept
/// for the front end's call). Answers whether one was running.
#[tauri::command]
pub fn kernel_cancel(id: u64) -> bool {
    let _ = id;
    carve_core::progress::request_stop()
}

/// Whether a command is running and how far it has got, 0 to 1.
#[tauri::command]
pub fn kernel_progress() -> carve_core::progress::Status {
    carve_core::progress::status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_replies_carry_the_json_tag() {
        let framed = frame(KernelReply::Json("{\"ok\":true}".to_string()));
        assert_eq!(&framed[..TAG_LEN], TAG_JSON);
        assert_eq!(&framed[TAG_LEN..], b"{\"ok\":true}");
    }

    #[test]
    fn byte_replies_carry_the_blob_tag() {
        let framed = frame(KernelReply::Bytes(vec![1, 2, 3]));
        assert_eq!(&framed[..TAG_LEN], TAG_BLOB);
        assert_eq!(&framed[TAG_LEN..], &[1, 2, 3]);
    }

    #[test]
    fn host_answers_ping() {
        let host = KernelHost::new();
        let framed = host.call("ping", "").expect("ping answers");
        let text = String::from_utf8(framed[TAG_LEN..].to_vec()).expect("reply is UTF-8");
        assert!(text.contains("\"pong\":true"), "{text}");
    }

    #[test]
    fn host_returns_kernel_errors_inside_the_envelope() {
        let host = KernelHost::new();
        let framed = host.call("no.such.command", "").expect("host answers");
        let text = String::from_utf8(framed[TAG_LEN..].to_vec()).expect("reply is UTF-8");
        assert!(text.contains("\"ok\":false"), "{text}");
        assert!(text.contains("unknown_command"), "{text}");
    }
}
