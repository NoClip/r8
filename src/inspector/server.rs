//! Safe Rust TCP Server and WebSocket RFC 6455 framing for Chrome DevTools Protocol (CDP).
//!
//! Provides discovery endpoints (`/json`, `/json/version`) and WebSocket transport
//! allowing direct connection from Google Chrome DevTools (`devtools://...`).

use super::session::InspectorSession;
use crate::builtins::base64::base64_encode;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};

/// Pure safe Rust SHA-1 implementation (RFC 3174) for WebSocket handshake calculation.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let msg_len_bits = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    // Append 0x80 byte
    msg.push(0x80);
    // Pad with zeros until len % 64 == 56
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    // Append length in bits as 64-bit big-endian
    msg.extend_from_slice(&msg_len_bits.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

/// Computes the `Sec-WebSocket-Accept` header response value according to RFC 6455.
pub fn compute_websocket_accept(key: &str) -> String {
    const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let combined = format!("{}{}", key.trim(), WS_GUID);
    let digest = sha1(combined.as_bytes());
    base64_encode(&digest)
}

/// Encodes a UTF-8 text message into an unmasked WebSocket frame (server-to-client).
pub fn encode_websocket_frame(text: &str) -> Vec<u8> {
    let payload = text.as_bytes();
    let len = payload.len();
    let mut frame = Vec::with_capacity(len + 10);

    // Byte 0: FIN (0x80) | opcode 0x01 (text) = 0x81
    frame.push(0x81);

    // Byte 1: Mask (0) | payload length
    if len <= 125 {
        frame.push(len as u8);
    } else if len <= 0xFFFF {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(payload);
    frame
}

/// Decodes a client WebSocket frame (must be masked per RFC 6455).
/// Returns `Ok(Some((text, total_bytes_consumed)))` or `Ok(None)` if incomplete.
pub fn decode_websocket_frame(bytes: &[u8]) -> Result<Option<(String, usize)>, String> {
    if bytes.len() < 2 {
        return Ok(None);
    }

    let b0 = bytes[0];
    let opcode = b0 & 0x0F;
    if opcode == 0x08 {
        // Connection Close
        return Err("WebSocket closed by client".to_string());
    }

    let b1 = bytes[1];
    let is_masked = (b1 & 0x80) != 0;
    let len_code = b1 & 0x7F;

    let mut header_len = 2;
    let payload_len: usize = if len_code <= 125 {
        len_code as usize
    } else if len_code == 126 {
        if bytes.len() < 4 { return Ok(None); }
        header_len += 2;
        u16::from_be_bytes([bytes[2], bytes[3]]) as usize
    } else {
        if bytes.len() < 10 { return Ok(None); }
        header_len += 8;
        u64::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9]]) as usize
    };

    let mask_key_len = if is_masked { 4 } else { 0 };
    let total_required = header_len + mask_key_len + payload_len;
    if bytes.len() < total_required {
        return Ok(None);
    }

    let mut payload = bytes[header_len + mask_key_len..total_required].to_vec();
    if is_masked {
        let mask = &bytes[header_len..header_len + 4];
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }

    let text = String::from_utf8(payload)
        .map_err(|e| format!("Invalid UTF-8 in WebSocket frame: {}", e))?;
    Ok(Some((text, total_required)))
}

/// Handles Chrome DevTools Protocol HTTP discovery requests or returns None if WebSocket request.
pub fn handle_http_discovery(request: &str, host: &str, port: u16) -> Option<String> {
    if request.starts_with("GET /json/version") {
        let body = format!(
            "{{\n  \"Browser\": \"v8/12.8.0 (Pure Safe Rust)\",\n  \"Protocol-Version\": \"1.3\"\n}}"
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        Some(resp)
    } else if request.starts_with("GET /json/list") || request.starts_with("GET /json ") {
        let body = format!(
            "[\n  {{\n    \"description\": \"d8\",\n    \"devtoolsFrontendUrl\": \"devtools://devtools/bundled/js_app.html?ws={}:{}/session\",\n    \"id\": \"1\",\n    \"title\": \"d8\",\n    \"type\": \"node\",\n    \"url\": \"file://\",\n    \"webSocketDebuggerUrl\": \"ws://{}:{}/session\"\n  }}\n]",
            host, port, host, port
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        Some(resp)
    } else {
        None
    }
}

/// A pure Safe Rust TCP server that provides the live DevTools Protocol interface.
pub struct InspectorServer {
    pub listener: TcpListener,
    pub host: String,
    pub port: u16,
}

impl InspectorServer {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let local_addr = listener.local_addr()?;
        Ok(Self {
            listener,
            host: local_addr.ip().to_string(),
            port: local_addr.port(),
        })
    }

    /// Accepts and handles incoming client interactions until client disconnects.
    pub fn handle_client(&self, mut stream: TcpStream, session: &mut InspectorSession) -> io::Result<()> {
        let mut buffer = [0u8; 4096];
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }

        let req_str = String::from_utf8_lossy(&buffer[..n]);

        // Check if HTTP discovery endpoint
        if let Some(resp) = handle_http_discovery(&req_str, &self.host, self.port) {
            stream.write_all(resp.as_bytes())?;
            stream.flush()?;
            return Ok(());
        }

        // Check if WebSocket upgrade
        if req_str.contains("Upgrade: websocket") || req_str.contains("upgrade: websocket") {
            let mut sec_key = "";
            for line in req_str.lines() {
                if let Some(pos) = line.to_lowercase().find("sec-websocket-key:") {
                    sec_key = line[pos + 18..].trim();
                    break;
                }
            }

            let accept_val = compute_websocket_accept(sec_key);
            let handshake = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                accept_val
            );
            stream.write_all(handshake.as_bytes())?;
            stream.flush()?;

            // WebSocket message loop
            let mut pending_buf = Vec::new();
            let mut temp = [0u8; 4096];
            loop {
                let bytes_read = match stream.read(&mut temp) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                pending_buf.extend_from_slice(&temp[..bytes_read]);

                while !pending_buf.is_empty() {
                    match decode_websocket_frame(&pending_buf) {
                        Ok(Some((msg, consumed))) => {
                            pending_buf.drain(..consumed);
                            let cdp_resp = session.dispatch(&msg);
                            let out_frame = encode_websocket_frame(&cdp_resp);
                            if stream.write_all(&out_frame).is_err() {
                                return Ok(());
                            }
                            let _ = stream.flush();
                        }
                        Ok(None) => break, // need more bytes
                        Err(_) => return Ok(()),
                    }
                }
            }
        }

        Ok(())
    }

    /// Runs the inspector server loop accepting incoming connections.
    pub fn run_loop(&self) {
        let mut session = InspectorSession::new();
        for stream in self.listener.incoming() {
            if let Ok(s) = stream {
                let _ = self.handle_client(s, &mut session);
            }
        }
    }
}
