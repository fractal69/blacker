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

use crate::{
    engine::engine::{Series, Timeframe}, master::state::{AppState, MasterState, ReplayStatus},
};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLockWriteGuard;
use tracing::info;

///
/// Request payload for deleting an existing series.
///
#[derive(Debug, Deserialize)]
pub struct Request {
    pub timeframe_id: String,
    pub id: String,
}
///
/// Response returned after attempting to delete a series.
///
#[derive(Serialize)]
pub struct Response {
    pub success: bool,
    pub message: String,
}
///
/// Deletes an existing series within a timeframe.
///
/// The config_id bump makes the engine rebuild its state from scratch
/// (without the removed series) and re-sync from this state.
///
pub async fn delete_series_handler(
    State(state): State<AppState>,
    Json(req): Json<Request>,
) -> (StatusCode, Json<Response>) {
    let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;

    if master.replay_status != ReplayStatus::Stopped {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "Cannot delete series while replay is running.".to_string(),
            }),
        );
    }

    let timeframe: &mut Timeframe = match master.engine_state.timeframes.get_mut(&req.timeframe_id) {
        Some(t) => t,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(Response {
                    success: false,
                    message: format!("Timeframe not found: {}", req.timeframe_id),
                }),
            );
        }
    };

    let removed: Option<Series> = timeframe.series.remove(&req.id);

    if removed.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(Response {
                success: false,
                message: format!("Series not found: {}", req.id),
            }),
        );
    }

    master.config_id = uuid::Uuid::now_v7().to_string();

    drop(master);

    let _ = state.publish_master_state().await;

    info!("Series deleted {} in timeframe {}", req.id, req.timeframe_id);

    (
        StatusCode::OK,
        Json(Response {
            success: true,
            message: "Series deleted successfully.".to_string(),
        }),
    )
}