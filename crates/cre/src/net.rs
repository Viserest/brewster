//! `root().net()`: requests a page makes to its backend, in the style of the `fetch` API.
//!
//! A page is sandboxed and has no sockets. A request is only *asked for*: it is queued, the
//! browser takes the queue (`cre_net_take`), decides whether to allow each request and
//! performs it, and hands the answer back (`cre_net_done`). The future returned here
//! completes then. Futures need something to run them: [`Root::spawn`](crate::Root::spawn)
//! is a small executor, polled whenever an answer arrives.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

use crate::{
    Method,
    codec::{Reader, Writer},
};

/// A request waiting for the browser to send it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetRequest {
    pub id: u32,
    pub method: Method,
    /// As the page wrote it: the browser resolves it against the page's address.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the server answered. Any status is a response (a `404` is not an error).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A status from 200 to 299.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text; bytes that are not UTF-8 become `U+FFFD`.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// The first header called `name`, whatever its case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// The request did not produce a response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkError {
    /// The browser refused it (another origin, say).
    Blocked(String),
    /// It was allowed but could not be completed (no connection, timeout ...).
    Failed(String),
    /// The page dropped the future before the answer came.
    Cancelled,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::Blocked(m) => write!(f, "blocked: {}", m),
            NetworkError::Failed(m) => write!(f, "network error: {}", m),
            NetworkError::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for NetworkError {}

pub type NetResult = Result<Response, NetworkError>;

// ── messages between the page and the browser ──────────────────────

pub fn encode_requests(reqs: &[NetRequest]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(reqs.len());
    for r in reqs {
        w.u32(r.id as usize);
        w.u8(r.method.code());
        w.str(&r.url);
        w.u32(r.headers.len());
        for (k, v) in &r.headers {
            w.str(k);
            w.str(v);
        }
        w.bytes(&r.body);
    }
    w.0
}

pub fn decode_requests(bytes: &[u8]) -> Result<Vec<NetRequest>, String> {
    let mut r = Reader::new(bytes);
    let n = r.count(4 + 1 + 4 + 4 + 4)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u32()? as u32;
        let method = Method::from_code(r.u8()?).ok_or("unknown request method")?;
        let url = r.str()?;
        let nh = r.count(8)?;
        let mut headers = Vec::with_capacity(nh);
        for _ in 0..nh {
            headers.push((r.str()?, r.str()?));
        }
        out.push(NetRequest {
            id,
            method,
            url,
            headers,
            body: r.bytes()?,
        });
    }
    Ok(out)
}

pub fn encode_result(res: &NetResult) -> Vec<u8> {
    let mut w = Writer::new();
    match res {
        Ok(r) => {
            w.u8(0);
            w.u16(r.status);
            w.u32(r.headers.len());
            for (k, v) in &r.headers {
                w.str(k);
                w.str(v);
            }
            w.bytes(&r.body);
        }
        Err(NetworkError::Blocked(m)) => {
            w.u8(1);
            w.str(m);
        }
        Err(NetworkError::Failed(m)) => {
            w.u8(2);
            w.str(m);
        }
        Err(NetworkError::Cancelled) => w.u8(3),
    }
    w.0
}

pub fn decode_result(bytes: &[u8]) -> Result<NetResult, String> {
    let mut r = Reader::new(bytes);
    Ok(match r.u8()? {
        0 => {
            let status = r.u16()?;
            let n = r.count(8)?;
            let mut headers = Vec::with_capacity(n);
            for _ in 0..n {
                headers.push((r.str()?, r.str()?));
            }
            Ok(Response {
                status,
                headers,
                body: r.bytes()?,
            })
        }
        1 => Err(NetworkError::Blocked(r.str()?)),
        2 => Err(NetworkError::Failed(r.str()?)),
        3 => Err(NetworkError::Cancelled),
        k => return Err(format!("unknown response kind {}", k)),
    })
}

// ── the page's side ────────────────────────────────────────────────

#[derive(Default)]
struct Slot {
    result: Option<NetResult>,
    waker: Option<Waker>,
}

#[derive(Default)]
struct State {
    next_id: u32,
    outbox: Vec<NetRequest>,
    slots: HashMap<u32, Slot>,
}

/// The page's way to the network. Cheap to clone; every clone shares the same queue.
#[derive(Clone, Default)]
pub struct Net(Rc<RefCell<State>>);

impl Net {
    pub(crate) fn new() -> Net {
        Net::default()
    }

    /// Queues a request with headers. The request starts when the browser next collects
    /// the queue, whether or not the future is polled (as with `fetch`).
    pub fn send(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> ResponseFuture {
        let mut s = self.0.borrow_mut();
        s.next_id += 1;
        let id = s.next_id;
        s.slots.insert(id, Slot::default());
        s.outbox.push(NetRequest {
            id,
            method,
            url: url.to_string(),
            headers,
            body,
        });
        ResponseFuture {
            id,
            net: self.clone(),
        }
    }

    pub fn get(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Get, url, Vec::new(), body)
    }
    pub fn head(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Head, url, Vec::new(), body)
    }
    pub fn post(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Post, url, Vec::new(), body)
    }
    pub fn put(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Put, url, Vec::new(), body)
    }
    pub fn patch(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Patch, url, Vec::new(), body)
    }
    pub fn delete(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Delete, url, Vec::new(), body)
    }
    pub fn options(&self, url: &str, body: Vec<u8>) -> ResponseFuture {
        self.send(Method::Options, url, Vec::new(), body)
    }

    /// The requests queued since the last call.
    pub(crate) fn take(&self) -> Vec<NetRequest> {
        std::mem::take(&mut self.0.borrow_mut().outbox)
    }

    /// The browser's answer. Wakes whoever waits for it; an answer for a request nobody
    /// waits for any more is dropped.
    pub(crate) fn complete(&self, id: u32, result: NetResult) {
        let waker = {
            let mut s = self.0.borrow_mut();
            match s.slots.get_mut(&id) {
                Some(slot) => {
                    slot.result = Some(result);
                    slot.waker.take()
                }
                None => None,
            }
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}

/// Completes with the server's [`Response`], or a [`NetworkError`].
pub struct ResponseFuture {
    id: u32,
    net: Net,
}

impl Future for ResponseFuture {
    type Output = NetResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<NetResult> {
        let mut s = self.net.0.borrow_mut();
        let Some(slot) = s.slots.get_mut(&self.id) else {
            return Poll::Ready(Err(NetworkError::Cancelled));
        };
        match slot.result.take() {
            Some(r) => {
                s.slots.remove(&self.id);
                Poll::Ready(r)
            }
            None => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl Drop for ResponseFuture {
    fn drop(&mut self) {
        let mut s = self.net.0.borrow_mut();
        s.slots.remove(&self.id);
        // Not sent yet: do not send it.
        let id = self.id;
        s.outbox.retain(|r| r.id != id);
    }
}
