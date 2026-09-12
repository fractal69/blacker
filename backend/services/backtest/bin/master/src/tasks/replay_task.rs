// BLACKER
// Copyright (C) 2026 Juan José Caballero Rey
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::master::state::{AppState, MasterState, ReplayStatus, Tick};
use crate::snapshot::save_snapshot;
use anyhow::{Context, Result};
use producer::SendFuture;
use pulsar::ProducerOptions;
use pulsar::{Error as PulsarError, Pulsar, TokioExecutor};
use pulsar::{Producer, SerializeMessage, producer};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLockReadGuard, RwLockWriteGuard};
use tokio::time::sleep;
use tracing::{info, warn};

//----------------------------------------------------------------------------------------------------------------------
// IMPLEMENTATION
//----------------------------------------------------------------------------------------------------------------------

///
/// Defines the replay state machine.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayStep {
    ///
    /// Publishes the next tick to Pulsar.
    ///
    PublishTick,
    ///
    /// Waits for the engine to process the tick.
    ///
    WaitEngine,
    ///
    /// Persists the current replay state.
    ///
    Persist,
}
///
/// Represents a serializable trade tick.
///
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickMessage {
    ///
    /// Unique tick identifier.
    ///
    pub id: u64,
    ///
    /// Tick timestamp.
    ///
    pub time: u64,
    ///
    /// Trade price.
    ///
    pub price: u64,
    //
    /// Trade quantity.
    ///
    pub qty: u64,
    ///
    /// Indicates whether the buyer was the maker.
    ///
    pub is_buyer_maker: u8,
}
///
/// Converts a [`Tick`] into a [`TickMessage`].
///
impl From<&Tick> for TickMessage {
    fn from(tick: &Tick) -> Self {
        Self {
            id: tick.id,
            time: tick.time,
            price: tick.price,
            qty: tick.qty,
            is_buyer_maker: tick.is_buyer_maker,
        }
    }
}
///
/// Represents a batch of trade ticks published to Pulsar.
///
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickBatchMessage {
    ///
    /// Unique identifier for the replay boot.
    ///
    pub boot_id: String,
    ///
    /// Hash identifying the replay configuration.
    ///
    pub config_id: String,
    ///
    /// Index of the first tick in the batch.
    ///
    pub first_tick_index: usize,
    ///
    /// Ticks included in the batch.
    ///
    pub ticks: Vec<TickMessage>,
}

impl SerializeMessage for TickBatchMessage {
    ///
    /// Serializes the tick batch into a MessagePack payload for Pulsar.
    ///
    fn serialize_message(input: Self) -> Result<producer::Message, PulsarError> {
        let payload: Vec<u8> = rmp_serde::to_vec(&input)
            .map_err(|e: rmp_serde::encode::Error| PulsarError::Custom(e.to_string()))?;

        Ok(producer::Message {
            payload,
            ..Default::default()
        })
    }
}

//----------------------------------------------------------------------------------------------------------------------
// REPLAY MACHINE
//----------------------------------------------------------------------------------------------------------------------

///
/// Runs the replay state machine until the replay is stopped.
///
async fn run_replay(state: AppState, producer: &mut Producer<TokioExecutor>) -> Result<()> {
    info!("Running replay task...");

    loop {
        //sleep(Duration::from_millis(1_000)).await; //DEBUG
        //
        // Check whether the master is ready to publish.
        //
        {
            let master: RwLockReadGuard<'_, MasterState> = state.master.read().await;

            if !master.can_publish() {
                sleep(Duration::from_millis(1_000)).await;
                continue;
            }
        }
        //
        // Read the current replay step.
        //
        let replay_step: ReplayStep = {
            let master: RwLockReadGuard<'_, MasterState> = state.master.read().await;
            master.replay_step
        };

        match replay_step {
            ReplayStep::PublishTick => {
                //
                // Acquire a read lock on the master state.
                //
                let (config_id, first_tick_index, batch_size, message) = {
                    let master: RwLockReadGuard<'_, MasterState> = state.master.read().await;
                    //
                    // Get the next batch of ticks to replay.
                    //
                    let ticks: &[Tick] = master.tick_batch(state.replay_batch_size);
                    //
                    // Stop the replay when there are no more ticks.
                    //
                    if ticks.is_empty() {
                        drop(master);

                        let mut master: RwLockWriteGuard<'_, MasterState> =
                            state.master.write().await;
                        master.replay_status = ReplayStatus::Stopped;

                        info!("Replay stopped no more ticks");

                        continue;
                    }

                    let batch_size: usize = ticks.len();
                    //
                    // Build the message to publish to Pulsar.
                    //
                    let message: TickBatchMessage = TickBatchMessage {
                        boot_id: state.boot_id.clone(),
                        config_id: master.config_id.clone(),
                        first_tick_index: master.tick_index,
                        ticks: ticks.iter().map(TickMessage::from).collect(),
                    };

                    (
                        master.config_id.clone(),
                        master.tick_index,
                        batch_size,
                        message,
                    )
                };
                //
                // Send the tick batch to Pulsar without blocking the producer.
                //
                let send_future: SendFuture = match producer.send_non_blocking(message).await {
                    Ok(f) => f,
                    Err(e) => {
                        warn!(
                            tick_index = first_tick_index,
                            error = %e,
                            "Producer send failed (non-fatal), will retry."
                        );
                        continue;
                    }
                };
                //
                // Wait for the producer to confirm the message.
                //
                match send_future.await {
                    Ok(_receipt) => {
                        if first_tick_index % 100_000 == 0 {
                            info!(
                                tick_index = first_tick_index,
                                batch_size = batch_size,
                                "Tick batch published."
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            tick_index = first_tick_index,
                            error = %e,
                            "Producer receipt failed (non-fatal), will retry."
                        );
                        continue;
                    }
                }
                //
                // Wait for the engine to process the published batch.
                //
                let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;

                if master.config_id != config_id {
                    warn!("config_id changed while publishing TickBatchMessage.");
                    continue;
                }

                master.replay_step = ReplayStep::WaitEngine;
            }

            ReplayStep::WaitEngine => {
                //
                // Wait for the engine to finish processing the current batch.
                //
                state.engine_notify.notified().await;
                //
                // Move to the persist state.
                //
                let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;
                master.replay_step = ReplayStep::Persist;
            }

            ReplayStep::Persist => {
                let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;
                //
                // Calculate the number of ticks remaining to process.
                //
                let remaining: usize = master.tick_data.len() - master.tick_index;
                let processed: usize = remaining.min(state.replay_batch_size);
                //
                // Advance the replay position by the number of processed ticks.
                //
                master.tick_index += processed;
                master.replay_step = ReplayStep::PublishTick;
                //
                // Save a snapshot every one million processed ticks.
                //
                if master.tick_index % 1_000_000 == 0 {
                    save_snapshot(&master).await?;
                }
                //
                // Drop the unused master lock.
                //
                drop(master);
                //
                // Ack engine consumer and publish the updated master state.
                //
                state.engine_ack_notify.notify_one();
                state.publish_master_state().await?;
            }
        }
    }

    Ok(())
}

//----------------------------------------------------------------------------------------------------------------------
// RUN
//----------------------------------------------------------------------------------------------------------------------
///
/// Starts the asynchronous replay worker.
///
pub async fn run(state: AppState, pulsar: Arc<Pulsar<TokioExecutor>>) -> Result<()> {
    //
    // Create the Pulsar producer.
    //
    let mut producer: Producer<TokioExecutor> = pulsar
        .producer()
        .with_topic("persistent://public/default/master.tick")
        .with_options(ProducerOptions {
            batch_size: Some(0),
            batch_timeout: Some(Duration::from_millis(1)),
            block_queue_if_full: true,
            ..Default::default()
        })
        .build()
        .await
        .context("Failed to create Pulsar producer.")?;

    //
    // Run the replay loop.
    //
    run_replay(state, &mut producer)
        .await
        .context("Replay state machine failed.")?;

    Ok(())
}
