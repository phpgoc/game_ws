use serde::{Serialize, Deserialize};
use typeshare::typeshare;

/// 范围选择参数 — 告知前端这是一个 slider，范围 [min, max]，默认值 default。
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameParamRange {
    pub default: i32,
    pub min: i32,
    pub max: i32,
}

/// 枚举选择参数 — 告知前端这是一个 radio/dropdown，选项为 options，默认索引 default。
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameParamEnum {
    pub default: usize,
    pub options: Vec<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameParam {
    Range(GameParamRange),
    Enum(GameParamEnum),
}