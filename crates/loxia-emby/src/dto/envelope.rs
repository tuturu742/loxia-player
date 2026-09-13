//! ItemsResponse pagination envelope.

use serde::Deserialize;

use super::item::BaseItemDto;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsResponse {
    #[serde(default)]
    pub items: Vec<BaseItemDto>,
    #[serde(default)]
    pub total_record_count: usize,
    #[serde(default)]
    pub start_index: usize,
}
