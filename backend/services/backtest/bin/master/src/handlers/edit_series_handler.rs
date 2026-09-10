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
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLockWriteGuard;
use tracing::info;


///
/// Request payload for editing an existing series.
///
#[derive(Debug, Deserialize)]
pub struct Request {
    pub timeframe_id: String,
    pub id: String,
    pub overlay: bool,
    pub params: HashMap<String, Value>,
    pub affects_compute: bool,
}
///
/// Response returned after attempting to edit a series.
///
#[derive(Serialize)]
pub struct Response {
    pub success: bool,
    pub message: String,
}
///
/// Edits an existing series within a timeframe.
///
/// When `affects_compute` is true the series accumulated data
/// (live, history, closed state) is wiped from the state; the
/// config_id bump makes the engine reset and re-sync from this state.
///
pub async fn edit_series_handler(
    State(state): State<AppState>,
    Json(req): Json<Request>,
) -> (StatusCode, Json<Response>) {
    let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;

    if master.replay_status != ReplayStatus::Stopped {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "Cannot edit series while replay is running.".to_string(),
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

    if !timeframe.series.contains_key(&req.id) {
        return (
            StatusCode::NOT_FOUND,
            Json(Response {
                success: false,
                message: format!("Series not found: {}", req.id),
            }),
        );
    }


    let series: &mut Series = match timeframe.series.get_mut(&req.id) {
        Some(s) => s,
        None => unreachable!(),
    };

    series.overlay = req.overlay;
    series.params = req.params;

    if req.affects_compute {
        series.extra = None;
    }

    master.config_id = uuid::Uuid::now_v7().to_string();

    drop(master);

    let _ = state.publish_master_state().await;

    info!("Series edited {} in timeframe {}", req.id, req.timeframe_id);

    (
        StatusCode::OK,
        Json(Response {
            success: true,
            message: "Series edited successfully.".to_string(),
        }),
    )
}
