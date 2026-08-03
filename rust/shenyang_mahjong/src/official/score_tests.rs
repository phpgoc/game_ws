use super::player_scores_for_official_users;
use share_type_public::games::shenyang_mahjong::WsShenyangMahjongScoreChange;

#[test]
fn official_player_scores_keep_human_losses_when_ai_wins() {
    let scores = player_scores_for_official_users(
        &[
            WsShenyangMahjongScoreChange {
                position: 0,
                score: -12,
            },
            WsShenyangMahjongScoreChange {
                position: 1,
                score: 12,
            },
            WsShenyangMahjongScoreChange {
                position: 2,
                score: 0,
            },
        ],
        |position| match position {
            0 => Some(101),
            2 => Some(103),
            _ => None,
        },
    );

    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].user_id, 101);
    assert_eq!(scores[0].score, -12);
    assert_eq!(scores[1].user_id, 103);
    assert_eq!(scores[1].score, 0);
}
