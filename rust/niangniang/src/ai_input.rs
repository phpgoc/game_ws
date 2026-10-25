//! 只包含本人可见信息，不允许智能策略接收裁判 Room。
use share_type_public::games::niangniang::NiangniangStateEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicTurn {
    Play { position: usize, cards: Vec<i32> },
    Pass { position: usize },
}

pub(crate) struct AiObservation<'a> {
    pub state: NiangniangStateEvent,
    pub history: &'a [PublicTurn],
}
