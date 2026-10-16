use super::*;
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};
use ws_common::CommonGameState;

fn ai_state() -> Arc<StdMutex<LandlordLoopState>> {
    let mut common = CommonGameState::new();
    for position in 0..3 {
        common.add_player(position, position as u64 + 1, &format!("P{position}"));
    }
    common.mark_ai_takeover_position(0);
    let mut state = LandlordLoopState::new(Arc::new(StdMutex::new(common)));
    state.phase = LandlordPhase::Play;
    state.landlord_position = Some(0);
    state.hands = HashMap::from([(0, vec![1, 2]), (1, vec![3, 4]), (2, vec![5, 6])]);
    Arc::new(StdMutex::new(state))
}

struct Release(Arc<(StdMutex<bool>, std::sync::Condvar)>);

impl Release {
    fn new() -> Self {
        Self(Arc::new((StdMutex::new(false), std::sync::Condvar::new())))
    }
    fn unblock(&self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        self.unblock();
    }
}

async fn controlled_calculation(
    state: &Arc<StdMutex<LandlordLoopState>>,
    slots: Arc<tokio::sync::Semaphore>,
    release: &Release,
) -> tokio::task::JoinHandle<Option<PreparedAiAction>> {
    let state = Arc::clone(state);
    let blocker = Arc::clone(&release.0);
    let (started, received) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        prepare_with(&state, slots, move |observation| {
            let _ = started.send(());
            let (lock, ready) = &*blocker;
            let _guard = ready
                .wait_while(lock.lock().unwrap(), |done| !*done)
                .unwrap();
            (None, Some(vec![observation.hand[0]]))
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), received)
        .await
        .unwrap()
        .unwrap();
    task
}

#[tokio::test(flavor = "current_thread")]
async fn calculation_releases_both_game_locks_and_keeps_the_runtime_responsive() {
    let state = ai_state();
    let release = Release::new();
    let task =
        controlled_calculation(&state, Arc::new(tokio::sync::Semaphore::new(1)), &release).await;
    {
        let state = state
            .try_lock()
            .expect("calculation must not hold game state");
        let common = state
            .base
            .try_lock()
            .expect("calculation must not hold common state");
        assert!(!common.action_received);
    }
    // 在单线程运行时，阻塞中的计算仍允许计时器前进。
    tokio::time::timeout(
        Duration::from_secs(1),
        tokio::time::sleep(Duration::from_millis(10)),
    )
    .await
    .unwrap();
    assert!(!task.is_finished());
    release.unblock();
    let prepared = task.await.unwrap().unwrap();
    assert_eq!(prepared.plan.play, Some(vec![1]));
    assert!(prepared.is_current(&state.lock().unwrap()));
}

#[tokio::test]
async fn cancelling_a_room_discards_work_but_retains_the_cpu_permit_until_it_finishes() {
    let state = ai_state();
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    let release = Release::new();
    let task = controlled_calculation(&state, Arc::clone(&slots), &release).await;
    state.lock().unwrap().request_stop();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert_eq!(slots.available_permits(), 0);
    release.unblock();
    let _permit = tokio::time::timeout(Duration::from_secs(1), slots.acquire())
        .await
        .unwrap()
        .unwrap();
    assert!(state.lock().unwrap().current_play.is_empty());
}

#[tokio::test]
async fn cancelled_work_waiting_for_a_cpu_slot_never_starts_inference() {
    let state = ai_state();
    let slots = Arc::new(tokio::sync::Semaphore::new(0));
    let called = Arc::new(AtomicBool::new(false));
    let future = prepare_with(&state, Arc::clone(&slots), {
        let called = Arc::clone(&called);
        move |_| {
            called.store(true, Ordering::SeqCst);
            (None, Some(vec![1]))
        }
    });
    tokio::pin!(future);
    tokio::select! {
        _ = &mut future => panic!("inference should be waiting for a slot"),
        _ = tokio::time::sleep(Duration::from_millis(10)) => {}
    }
    state
        .lock()
        .unwrap()
        .base
        .lock()
        .unwrap()
        .clear_ai_takeover_position(0);
    slots.add_permits(1);
    assert!(future.await.is_none());
    assert!(!called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn manual_reclaim_and_public_actions_during_inference_discard_the_result() {
    for manual_reclaim in [true, false] {
        let state = ai_state();
        let release = Release::new();
        let task =
            controlled_calculation(&state, Arc::new(tokio::sync::Semaphore::new(1)), &release)
                .await;
        {
            let state = state.lock().unwrap();
            if manual_reclaim {
                state.base.lock().unwrap().clear_ai_takeover_position(0);
            } else {
                state.set_action_received(true);
            }
        }
        release.unblock();
        assert!(task.await.unwrap().is_none());
        assert!(state.lock().unwrap().current_play.is_empty());
    }
}

#[tokio::test]
async fn changed_public_turns_pause_and_manual_reclaim_invalidate_prepared_actions() {
    let state = ai_state();
    let prepared = prepare_with(&state, Arc::new(tokio::sync::Semaphore::new(1)), |_| {
        (None, Some(vec![1]))
    })
    .await
    .unwrap();
    let mut state = state.lock().unwrap();
    assert!(prepared.is_current(&state));
    // 对手暗牌归属变化不影响观察，也不作为过期判断依据。
    state.hands.insert(1, vec![5, 6]);
    state.hands.insert(2, vec![3, 4]);
    assert!(prepared.is_current(&state));
    state.last_play_position = 1;
    state.last_play = vec![4];
    assert!(!prepared.is_current(&state));
    state.last_play_position = 0;
    state.last_play.clear();
    state.set_action_received(true);
    assert!(!prepared.is_current(&state));
    state.set_action_received(false);
    state.base.lock().unwrap().pause();
    assert!(!prepared.is_current(&state));
    state.base.lock().unwrap().paused = false;
    state.base.lock().unwrap().clear_ai_takeover_position(0);
    assert!(!prepared.is_current(&state));
}

#[tokio::test]
async fn public_entry_matches_the_formal_policy_and_rejects_illegal_calculation_results() {
    let state = ai_state();
    let expected = crate::ai::choose_play(&state.lock().unwrap(), 0);
    let prepared = prepare_ai_action(&state).await.unwrap();
    assert_eq!(prepared.plan.play.as_ref(), Some(&expected));
    assert!(
        prepare_with(&state, Arc::new(tokio::sync::Semaphore::new(1)), |_| (
            None,
            Some(vec![54])
        ))
        .await
        .is_none()
    );
    {
        let mut state = state.lock().unwrap();
        state.phase = LandlordPhase::CallLandlord;
        state.score = 2;
    }
    let expected = crate::ai::choose_bid(&state.lock().unwrap(), 0);
    assert_eq!(prepare_ai_action(&state).await.unwrap().bid, Some(expected));
    assert!(
        prepare_with(&state, Arc::new(tokio::sync::Semaphore::new(1)), |_| (
            Some(2),
            None
        ))
        .await
        .is_none()
    );
    assert!(
        prepare_with(&state, Arc::new(tokio::sync::Semaphore::new(1)), |_| (
            Some(4),
            None
        ))
        .await
        .is_none()
    );
}
