use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialPlayerSession {
    pub position: usize,
    pub session_id: String,
}

#[derive(Debug)]
pub struct OfficialMatchInfo {
    pub id: i32,
    pub user_ids_by_position: HashMap<usize, i32>,
}

impl OfficialMatchInfo {
    pub fn user_id(&self, position: usize) -> Option<i32> {
        self.user_ids_by_position.get(&position).copied()
    }
}

/// 每场比赛独立持有创建结果，旧任务完成后不会写回同名房间。
/// 结算可以等待结果，避免发牌早于数据库创建时漏记第一轮。
#[derive(Debug, Clone)]
pub struct OfficialMatchHandle {
    result: watch::Receiver<Option<Arc<OfficialMatchInfo>>>,
    finished: watch::Sender<bool>,
}

impl OfficialMatchHandle {
    pub(crate) fn spawn<End: Future<Output = ()> + Send + 'static>(
        runtime: &tokio::runtime::Handle,
        create: impl Future<Output = Option<(i32, HashMap<usize, i32>)>> + Send + 'static,
        finish: impl FnOnce(i32) -> End + Send + 'static,
    ) -> Self {
        let (complete, result) = watch::channel(None);
        let (finished, mut ending) = watch::channel(false);
        runtime.spawn(async move {
            if let Some((id, user_ids_by_position)) = create.await {
                complete.send_replace(Some(Arc::new(OfficialMatchInfo {
                    id,
                    user_ids_by_position,
                })));
                drop(complete);
                while !*ending.borrow_and_update() {
                    if ending.changed().await.is_err() {
                        break;
                    }
                }
                finish(id).await;
            }
        });
        Self { result, finished }
    }

    pub(crate) fn ready(info: OfficialMatchInfo) -> Self {
        let (_, result) = watch::channel(Some(Arc::new(info)));
        let (finished, _) = watch::channel(false);
        Self { result, finished }
    }

    /// 结束信号属于这场比赛；重复清理、延迟创建和同名新房间都不会改写目标。
    pub fn finish(&self) {
        self.finished.send_replace(true);
    }

    pub fn get(&self) -> Option<Arc<OfficialMatchInfo>> {
        self.result.borrow().clone()
    }

    pub async fn wait(&self) -> Option<Arc<OfficialMatchInfo>> {
        let mut result = self.result.clone();
        loop {
            if let Some(info) = result.borrow_and_update().clone() {
                return Some(info);
            }
            if result.changed().await.is_err() {
                return None;
            }
        }
    }
}
