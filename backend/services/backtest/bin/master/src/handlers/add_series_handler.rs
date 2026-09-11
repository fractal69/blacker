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
    engine::engine::{Series, Timeframe},
    master::state::{AppState, MasterState, ReplayStatus},
};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLockWriteGuard;
use tracing::info;

///
/// Request payload for adding a new series.
///
#[derive(Debug, Deserialize)]
pub struct Request {
    pub timeframe_id: String,

    pub id: String,
    pub kind: String,
    pub level: u8,
    pub primary: bool,
    pub overlay: bool,
    pub params: HashMap<String, Value>,
}
///
/// Response returned after attempting to add a series.
///
#[derive(Serialize)]
pub struct Response {
    pub success: bool,
    pub message: String,
}
///
/// Adds a new series to an existing timeframe.
///
pub async fn add_series_handler(
    State(state): State<AppState>,
    Json(req): Json<Request>,
) -> (StatusCode, Json<Response>) {
    //TODO: validate params.

    /*
    TODO: Validate Series UI const params:

        let series_params: HashMap<String, Value> = HashMap::from([
        ("label".to_string(), json!("Candlestick")),
        ("layer".to_string(), json!("background")),
        ("color".to_string(), json!("#1cdac4")),
        ("priceTagColor".to_string(), json!("#F23645")),
    ]);

     */

    let mut master: RwLockWriteGuard<'_, MasterState> = state.master.write().await;

    if master.replay_status != ReplayStatus::Stopped {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "Cannot add series while replay is running.".to_string(),
            }),
        );
    }

    if master.tick_index != 0 {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: "Cannot add series after the backtest has started.".to_string(),
            }),
        );
    }

    let timeframe: &mut Timeframe = match master.engine_state.timeframes.get_mut(&req.timeframe_id)
    {
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

    if timeframe.series.contains_key(&req.id) {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: format!("The specified series already exists: {}", req.id),
            }),
        );
    }

    if req.primary
        && timeframe
            .series
            .values()
            .any(|series: &Series| series.primary)
    {
        return (
            StatusCode::CONFLICT,
            Json(Response {
                success: false,
                message: format!(
                    "A primary series already exists in timeframe: {}",
                    req.timeframe_id
                ),
            }),
        );
    }

    let n: u32 = rand::RngExt::random::<u32>(&mut rand::rng());

    let series_id: String = format!("{}-{}-{}", req.id, req.timeframe_id, n);

    let series: Series = Series {
        id: series_id,
        kind: req.kind,
        level: req.level,
        primary: req.primary,
        overlay: req.overlay,
        params: req.params,
        extra: None,
    };

    timeframe.series.insert(series.id.clone(), series);

    master.config_id = uuid::Uuid::now_v7().to_string();

    drop(master);

    let _ = state.publish_master_state().await;

    info!("Series added {} to timeframe {}", req.id, req.timeframe_id);

    (
        StatusCode::CREATED,
        Json(Response {
            success: true,
            message: "Series added successfully.".to_string(),
        }),
    )
}
