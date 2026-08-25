//! 沈阳麻将的服务端规则、状态机和 WebSocket 适配层。
//!
//! 这个 crate 被两种运行方式复用：官方服务端会启用 `official` 特性，
//! 自建/测试服务端则使用本地的 AI fallback。牌局规则只依赖共享协议中的
//! 牌面和动作类型，避免把浏览器 UI 的状态反向带入服务端。

#[cfg(feature = "official")]
#[path = "../../../../ai/shenyang_mahjong/src/embedded/mod.rs"]
mod ai;
#[cfg(not(feature = "official"))]
#[path = "ai_fallback.rs"]
mod ai;
/// WebSocket 请求处理、动作校验和结算事件构造。
pub mod game;
/// 后台循环负责超时、托管和自动行动，不直接决定牌型是否合法。
mod game_loop;
/// 房间设置的默认值、范围校验和支付封顶配置。
pub mod game_setting;
/// 牌局状态、吃碰杠窗口和结算状态的领域模型。
pub mod game_state;
mod official;
mod rules;
/// 将麻将服务注册到通用 WebSocket runtime 的启动入口。
pub mod server;
