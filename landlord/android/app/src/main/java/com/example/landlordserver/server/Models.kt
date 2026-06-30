package com.example.landlordserver.server

import org.java_websocket.WebSocket

data class Player(
    var sessionId: Long,
    val name: String,
    var position: Int,
    var avatarUrl: String,
    var active: Boolean = true,
    var away: Boolean = false,
)

data class SessionState(
    val id: Long,
    val socket: WebSocket,
    var name: String? = null,
    var roomKey: String? = null,
    var position: Int? = null,
)

data class Room(
    val key: String,
    val players: MutableMap<Int, Player> = linkedMapOf(),
    val configs: MutableMap<String, Int> = mutableMapOf(
        "start_time" to 1,
        "settlement_time" to 5,
        "turn_time" to 20,
    ),
    var paused: Boolean = false,
    var game: LandlordGame? = null,
)

data class CallRecord(val position: Int, val score: Int)

data class LandlordGame(
    var phase: Int = LandlordPhase.START,
    var callPosition: Int = 0,
    var currentPosition: Int = 0,
    val hands: MutableMap<Int, MutableList<Int>> = mutableMapOf(),
    var hiddenCards: MutableList<Int> = mutableListOf(),
    var landlordPosition: Int? = null,
    var score: Int = 0,
    val callHistory: MutableList<CallRecord> = mutableListOf(),
    var lastPlayPosition: Int = 0,
    var lastPlay: MutableList<Int> = mutableListOf(),
)
