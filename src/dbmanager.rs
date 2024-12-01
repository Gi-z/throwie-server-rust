use crate::config;
use crate::handler::HandledMessage;
use crate::tsdb::TimescaleClient;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::sleep;

pub struct DatabaseTaskManager {
    append_batch_rx: Mutex<mpsc::Receiver<Vec<HandledMessage>>>,

    batch: Arc<Mutex<Vec<HandledMessage>>>,
    batch_size_limit: usize,
    batch_time: u64,

    db: Arc<Mutex<TimescaleClient>>,

    reset_timer_signal: Arc<Notify>,
    write_signal: Arc<Notify>,
}

impl DatabaseTaskManager {
    pub async fn new(append_batch_rx: mpsc::Receiver<Vec<HandledMessage>>) -> Self {
        let batch = Arc::new(Mutex::new(Vec::new()));
        let db = Arc::new(Mutex::new(TimescaleClient::new().await));

        let batch_size_limit = config::get().lock().unwrap().timescale.write_batch_size as usize;
        let batch_time = config::get().lock().unwrap().timescale.batch_time;

        let reset_timer_signal = Arc::new(Notify::new());
        let write_signal = Arc::new(Notify::new());

        return Self {
            append_batch_rx: Mutex::new(append_batch_rx),

            batch,
            batch_size_limit,
            batch_time,
            db,

            reset_timer_signal,
            write_signal,
        };
    }

    pub fn start_watcher_jobs(self: Arc<Self>) {
        let tasks: Vec<fn(Arc<Self>) -> Pin<Box<dyn Future<Output = ()> + Send>>> = vec![
            |this| Box::pin(async { this.wait_for_timer().await }),
            |this| Box::pin(async { this.wait_for_incoming_batch().await }),
        ];

        for task in tasks {
            tokio::spawn(task(self.clone()));
        }
    }

    async fn wait_for_timer(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(self.batch_time)) => {
                    println!("Batch write triggered by timer expiration.");
                    self.write_signal.notify_one();
                }
                _ = self.reset_timer_signal.notified() => {
                    println!("Batch timer reset.");
                }
            }
        }
    }

    async fn wait_for_incoming_batch(self: Arc<Self>) {
        loop {
            let mut append_batch_rx = self.append_batch_rx.lock().await;

            tokio::select! {
                Some(msg_vec) = append_batch_rx.recv() => {
                    let mut batch_handle = self.batch.lock().await;
                    batch_handle.extend(msg_vec);

                    if batch_handle.len() > self.batch_size_limit {
                        println!("Batch size exceeds limit, triggering write.");
                        self.write_signal.notify_one();
                    }
                }
                _ = self.write_signal.notified() => {
                    println!("Received batch write notification.");

                    let batch_to_write = {
                        let mut batch_handle = self.batch.lock().await;
                        if batch_handle.is_empty() {
                            println!("Ignoring empty batch write.");
                            continue;
                        }

                        // Clone and clear the batch, then drop the lock
                        let copy = batch_handle.clone();
                        batch_handle.clear();
                        copy
                    };

                    let self_clone = self.clone();
                    tokio::spawn(async move {
                        self_clone.write_batch_to_db(batch_to_write).await;
                    });
                }
            }
        }
    }

    async fn write_batch_to_db(self: Arc<Self>, batch_copy: Vec<HandledMessage>) {
        // Notify the reset timer signal only after a successful write
        self.reset_timer_signal.notify_one();

        let mut csi_msgs = Vec::with_capacity(batch_copy.len());
        let mut telemetry_msgs = Vec::with_capacity(batch_copy.len());

        // Split batch into its constituent batches
        for msg in batch_copy {
            match msg {
                HandledMessage::CSIStorage(m) => csi_msgs.push(m),
                HandledMessage::Telemetry(m) => telemetry_msgs.push(m),
            }
        }

        // Lock the DB client to issue the write
        let mut db_handle = self.db.lock().await;
        db_handle.write_csi_data_batch(&csi_msgs).await;
        db_handle.write_telemetry_batch(&telemetry_msgs).await
    }
}

pub async fn start_db_watcher(db_append_batch_rx: mpsc::Receiver<Vec<HandledMessage>>) {
    let db_watcher = Arc::new(DatabaseTaskManager::new(db_append_batch_rx).await);
    db_watcher.start_watcher_jobs();
}
