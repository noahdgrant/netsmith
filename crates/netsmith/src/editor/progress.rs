//! Lets a job that reports through a plain function (`fn(&str)`, as the router and `fab`
//! take) report to the page that asked for it: the job runs on one thread, and the thread
//! carries the way to say.

use std::cell::RefCell;

use netsmith_editor::Say;

thread_local! {
    static SAY: RefCell<Option<Say>> = const { RefCell::new(None) };
}

/// Sends what the job says to `say` until it is dropped.
pub struct Reporting;

impl Reporting {
    pub fn to(say: &Say) -> Reporting {
        SAY.with(|s| *s.borrow_mut() = Some(say.clone()));
        Reporting
    }
}

impl Drop for Reporting {
    fn drop(&mut self) {
        SAY.with(|s| *s.borrow_mut() = None);
    }
}

/// What `RouteOptions::progress` and `FabOptions::progress` are set to.
pub fn forward(message: &str) {
    SAY.with(|s| {
        if let Some(say) = &*s.borrow() {
            say(message);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn a_job_reports_to_the_say_of_its_thread_and_only_while_it_runs() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&heard);
        let say: Say = Arc::new(move |m: &str| sink.lock().unwrap().push(m.to_string()));
        forward("before");
        {
            let _reporting = Reporting::to(&say);
            forward("routing 3 connections");
            std::thread::spawn(|| forward("another thread"))
                .join()
                .unwrap();
        }
        forward("after");
        assert_eq!(*heard.lock().unwrap(), ["routing 3 connections"]);
    }
}
