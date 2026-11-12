//! The pages that are open, for `/api/view` (P8a of issue 430): the way to each page's socket,
//! which of them had the focus last, and the asks that wait for a page's answer.
//!
//! **One page is driven: the one focused last.** Every socket is a page here from its handshake
//! until it closes. A page says `focus` when its window gets the focus, and on connecting if it
//! has it; a page that never said so counts as focused when it connected. An ask goes to the page
//! with the latest of those, so an agent drives the window a person last looked at, and a page
//! opened later (a person's new tab) takes over. The other pages are not told.
//!
//! **An ask is a round trip.** The server sends the page `view` or `view_picture` with an id, the
//! page answers `view_state`, `view_error` or a binary frame with the id, and the route answers
//! then. A page that closes takes its asks with it (they end at once, with the reason), and one
//! that does not answer in time is left: its late answer is dropped.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{mpsc, oneshot};

use crate::api::{ServerMessage, ViewState};

/// What a page answered.
#[derive(Debug)]
pub enum PageAnswer {
    State(Box<ViewState>),
    Picture(Vec<u8>),
    Error { status: u16, message: String },
}

#[derive(Default)]
pub struct Pages {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// A clock that counts connections and focus changes.
    clock: u64,
    next_ask: u32,
    open: Vec<Open>,
    /// Each ask waiting: the page it went to and where its answer goes.
    asks: HashMap<u32, (u64, oneshot::Sender<PageAnswer>)>,
}

struct Open {
    id: u64,
    focused: u64,
    tell: mpsc::UnboundedSender<String>,
}

/// A page's place among the open pages, given up when the socket ends.
pub struct Seat {
    pages: Arc<Pages>,
    pub id: u64,
}

impl Drop for Seat {
    fn drop(&mut self) {
        let mut inner = self.pages.lock();
        inner.open.retain(|p| p.id != self.id);
        // Its asks end now: the senders dropped tell the routes that wait.
        inner.asks.retain(|_, (page, _)| *page != self.id);
    }
}

impl Pages {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A page connected: its seat, and what the server tells it alone.
    pub fn open(self: &Arc<Self>) -> (Seat, mpsc::UnboundedReceiver<String>) {
        let (tell, told) = mpsc::unbounded_channel();
        let mut inner = self.lock();
        inner.clock += 1;
        let id = inner.clock;
        inner.open.push(Open {
            id,
            focused: id,
            tell,
        });
        (
            Seat {
                pages: Arc::clone(self),
                id,
            },
            told,
        )
    }

    /// The page `id` has the focus now.
    pub fn focus(&self, id: u64) {
        let mut inner = self.lock();
        inner.clock += 1;
        let now = inner.clock;
        if let Some(p) = inner.open.iter_mut().find(|p| p.id == id) {
            p.focused = now;
        }
    }

    /// Send the page focused last the message `make` builds from the ask's id; the answer comes
    /// on the receiver. None when no page is open.
    pub fn ask(
        &self,
        make: impl FnOnce(u32) -> ServerMessage,
    ) -> Option<(u32, oneshot::Receiver<PageAnswer>)> {
        let mut inner = self.lock();
        inner.next_ask = inner.next_ask.wrapping_add(1);
        let ask = inner.next_ask;
        let text = serde_json::to_string(&make(ask)).ok()?;
        // The latest focus first; a page whose socket is going is passed over.
        let mut order: Vec<(u64, u64)> = inner.open.iter().map(|p| (p.focused, p.id)).collect();
        order.sort_unstable_by(|a, b| b.cmp(a));
        for (_, id) in order {
            let Some(page) = inner.open.iter().find(|p| p.id == id) else {
                continue;
            };
            if page.tell.send(text.clone()).is_ok() {
                let (answer, answered) = oneshot::channel();
                inner.asks.insert(ask, (id, answer));
                return Some((ask, answered));
            }
        }
        None
    }

    /// A page answered the ask `ask`. An answer nobody waits for is dropped.
    pub fn answer(&self, from: u64, ask: u32, answer: PageAnswer) {
        let mut inner = self.lock();
        if inner.asks.get(&ask).is_some_and(|(page, _)| *page == from)
            && let Some((_, to)) = inner.asks.remove(&ask)
        {
            let _ = to.send(answer);
        }
    }

    /// The route gave up on `ask`.
    pub fn forget(&self, ask: u32) {
        self.lock().asks.remove(&ask);
    }
}
