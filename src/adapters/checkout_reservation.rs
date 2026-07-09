use std::{sync::Arc, time::Duration};

use chrono::Utc;
use tokio::time::interval;
use tracing::{error, info};

use crate::port::donation::DonationRepository;

const RESERVATION_EXPIRY_POLL_SECONDS: u64 = 15;

pub async fn run_checkout_reservation_expiry_worker(repository: Arc<dyn DonationRepository>) {
    let mut ticker = interval(Duration::from_secs(RESERVATION_EXPIRY_POLL_SECONDS));

    loop {
        ticker.tick().await;

        match repository.expire_checkout_reservations(Utc::now()).await {
            Ok(expired) if expired > 0 => {
                info!(expired, "expired abandoned checkout reservations");
            }
            Ok(_) => {}
            Err(error) => {
                error!(%error, "checkout reservation expiry worker pass failed");
            }
        }
    }
}
