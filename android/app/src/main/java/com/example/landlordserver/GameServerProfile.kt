package com.example.langameserver

data class GameServerProfile(
    val id: String,
    val gameNameRes: Int,
    val serviceTitleRes: Int,
    val port: Int,
)

object GameServerProfiles {
    val LANDLORD = GameServerProfile(
        id = "landlord",
        gameNameRes = R.string.game_landlord,
        serviceTitleRes = R.string.title_landlord_ws_service,
        port = 9001,
    )
    val SHENYANG_MAHJONG = GameServerProfile(
        id = "shenyang_mahjong",
        gameNameRes = R.string.game_shenyang_mahjong,
        serviceTitleRes = R.string.title_shenyang_mahjong_ws_service,
        port = 9002,
    )
    val HOLDEM = GameServerProfile(
        id = "holdem",
        gameNameRes = R.string.game_holdem,
        serviceTitleRes = R.string.title_holdem_ws_service,
        port = 9003,
    )
    val TRACTOR = GameServerProfile(
        id = "tractor",
        gameNameRes = R.string.game_tractor,
        serviceTitleRes = R.string.title_tractor_ws_service,
        port = 9004,
    )
    val P2P = GameServerProfile(
        id = "p2p",
        gameNameRes = R.string.game_p2p,
        serviceTitleRes = R.string.title_p2p_ws_service,
        port = 9005,
    )
}

val ActiveGameServer: GameServerProfile = when (BuildConfig.GAME_ID) {
    GameServerProfiles.SHENYANG_MAHJONG.id -> GameServerProfiles.SHENYANG_MAHJONG
    GameServerProfiles.HOLDEM.id -> GameServerProfiles.HOLDEM
    GameServerProfiles.TRACTOR.id -> GameServerProfiles.TRACTOR
    GameServerProfiles.P2P.id -> GameServerProfiles.P2P
    else -> GameServerProfiles.LANDLORD
}.copy(port = BuildConfig.SERVER_PORT)
