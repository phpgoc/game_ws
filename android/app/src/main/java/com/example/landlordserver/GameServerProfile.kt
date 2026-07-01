package com.example.landlordserver

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
}

val ActiveGameServer: GameServerProfile = GameServerProfiles.LANDLORD
