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
    master::state::{AppState, MasterState, ReplayStatus},
    engine::engine::{Series, Timeframe},
};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::sync::RwLockWriteGuard;
use tracing::info;
use uuid::Uuid;

/// Request payload for adding a new timeframe.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: String,
}

/// Response returned after attempting to add a timeframe.
#[derive(Serialize)]
pub struct Response {
    pub success: bool,
    pub message: String,
}

/// Adds a new timeframe to the master engine_state.
pub async fn add_timeframe_handler(
    State(state): State<AppState>,
    Json(req): Json<Request>,
) -> (StatusCode, Json<Response>) {

    //TODO: validate params.

    let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;

    if master.replay_status != ReplayStatus::Stopped {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "Cannot add timeframe while replay is running.".to_string(),
            }),
        );
    }

    if master.engine_state.timeframes.contains_key(&req.id) {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "The specified timeframe already exists.".to_string(),
            }),
        );
    }

    let timeframe_ms: u64 = match Timeframe::timeframe_ms(&req.id) {
        Some(ms) => ms,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(Response {
                    success: false,
                    message: format!("Invalid timeframe: {}", req.id),
                }),
            );
        }
    };

    let n: u32 = rand::RngExt::random::<u32>(&mut rand::rng());

    let series_id: String = format!("candlestick-{}-{}", req.id, n);

    let series_params: HashMap<String, Value> = HashMap::from([
        ("label".to_string(), json!("Candlestick")),
        ("layer".to_string(), json!("background")),
        ("color".to_string(), json!("#1cdac4")),
        ("priceTagColor".to_string(), json!("#F23645")),
        ("bullColor".to_string(), json!("#089981")),
        ("bearColor".to_string(), json!("#F23645"))
    ]);

    let timeframe: Timeframe = Timeframe {
        id: req.id.clone(),
        series: HashMap::from([(
            series_id.clone(),
            Series {
                id: series_id,
                kind: "Candlestick".to_string(),
                level: 0,
                primary: true,
                overlay: false,
                params: series_params,
                extra: None,
            },
        )]),
        timeframe_ms,
        live: None,
        closed: None,
        is_new: false,
        is_closed: false
    };

    master
        .engine_state
        .timeframes
        .insert(timeframe.id.clone(), timeframe);

    master.config_id = Uuid::now_v7().to_string();

    drop(master);

    let _ = state.publish_master_state().await;

    info!("Timeframe added {}", req.id);

    (
        StatusCode::CREATED,
        Json(Response {
            success: true,
            message: "Timeframe added successfully.".to_string(),
        }),
    )
}
