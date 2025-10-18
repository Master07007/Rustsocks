use std::time::Duration;

use tokio::time::Interval;

pub struct Checker {
    interval: Interval,
    need_check: bool,
}

impl Checker {
    pub fn new(period: Duration) -> Self {
        Self {
            interval: tokio::time::interval(period),
            need_check: false,
        }
    }

    #[inline]
    pub fn activate(&mut self) {
        if !self.need_check {
            self.need_check = true;
        }
    }

    pub async fn wait(&mut self) {
        if self.need_check {
            self.interval.tick().await;
            self.need_check = false;
        } else {
            futures::future::pending().await
        }
    }
}
