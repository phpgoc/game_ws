package com.example.landlordserver.server

import org.java_websocket.WebSocket
import org.java_websocket.handshake.ClientHandshake
import org.java_websocket.server.WebSocketServer
import org.json.JSONObject
import java.net.InetSocketAddress
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong
import kotlin.random.Random

class LandlordWebSocketServer(
    address: InetSocketAddress,
    private val onStateChanged: () -> Unit,
) : WebSocketServer(address) {
    private val nextSessionId = AtomicLong(1)
    private val sessions = ConcurrentHashMap<WebSocket, SessionState>()
    private val rooms = ConcurrentHashMap<String, Room>()
    private val executor = Executors.newSingleThreadExecutor()
    @Volatile
    private var shuttingDown = false

    override fun onOpen(conn: WebSocket, handshake: ClientHandshake) {
        sessions[conn] = SessionState(nextSessionId.getAndIncrement(), conn)
        onStateChanged()
    }

    override fun onClose(conn: WebSocket, code: Int, reason: String, remote: Boolean) {
        if (shuttingDown) return
        executor.execute {
            val session = sessions.remove(conn) ?: return@execute
            markDisconnected(session)
            onStateChanged()
        }
    }

    override fun onMessage(conn: WebSocket, message: String) {
        if (shuttingDown) return
        executor.execute {
            val session = sessions[conn] ?: return@execute
            val request = runCatching { JSONObject(message) }.getOrNull() ?: return@execute
            handleRequest(session, request)
        }
    }

    override fun onError(conn: WebSocket?, ex: Exception) {
        onStateChanged()
    }

    override fun onStart() {
        connectionLostTimeout = 30
        onStateChanged()
    }

    fun clientCount(): Int = sessions.size

    fun roomCount(): Int = rooms.size

    override fun stop(timeout: Int) {
        shuttingDown = true
        executor.shutdownNow()
        try {
            super.stop(timeout)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
        rooms.values.forEach { it.game = null }
        rooms.clear()
        sessions.clear()
    }

    private fun handleRequest(session: SessionState, request: JSONObject) {
        when (request.optInt("route", -1)) {
            Routes.JOIN -> handleJoin(session, request.optJsonObject("data") ?: JSONObject())
            Routes.QUIT -> handleQuit(session)
            Routes.DISBAND -> handleDisband(session)
            Routes.SETTING -> handleSetting(session, request.optJsonObject("data") ?: JSONObject())
            Routes.START -> handleStart(session)
            Routes.CALL_LANDLORD -> handleCallLandlord(session, request.optJsonObject("data") ?: JSONObject())
            Routes.PLAY -> handlePlay(session, request.optJsonObject("data") ?: JSONObject())
            Routes.AWAY -> handleAway(session)
            Routes.BACK -> handleBack(session)
            else -> sendWithoutData(session, request.optInt("route", -1), WsResponseCode.NOT_IN_RANGE)
        }
    }

    private fun handleJoin(session: SessionState, data: JSONObject) {
        val gameId = data.optIntOrNull("game_id")
        if (gameId != GameId.LANDLORD) {
            sendWithoutData(session, Routes.JOIN, WsResponseCode.WRONG_GAME)
            return
        }
        val name = data.optStringOrEmpty("name").trim()
        val password = data.optStringOrEmpty("password").trim()
        val avatarUrl = data.optStringOrEmpty("avatar_url")
        if (name.isEmpty() || password.isEmpty()) {
            sendWithoutData(session, Routes.JOIN, WsResponseCode.ERROR_FORMAT)
            return
        }
        if (session.roomKey != null) {
            if (session.roomKey == password && session.name == name) {
                sendJoinResponse(session, rooms[password], session.position ?: 0)
            } else {
                sendWithoutData(session, Routes.JOIN, WsResponseCode.NO_PERMISSION)
            }
            return
        }

        val room = rooms.getOrPut(password) { Room(password) }
        val existingSameName = room.players.values.firstOrNull { it.name == name }
        val position = when {
            existingSameName != null && existingSameName.active -> {
                sendWithoutData(session, Routes.JOIN, WsResponseCode.NO_PERMISSION)
                return
            }
            existingSameName != null -> existingSameName.position
            else -> firstFreePosition(room) ?: run {
                sendWithoutData(session, Routes.JOIN, WsResponseCode.NO_PERMISSION)
                return
            }
        }

        if (existingSameName != null) {
            existingSameName.sessionId = session.id
            existingSameName.avatarUrl = avatarUrl.ifBlank { existingSameName.avatarUrl }
            existingSameName.active = true
            existingSameName.away = false
        } else {
            room.players[position] = Player(session.id, name, position, avatarUrl)
        }
        session.name = name
        session.roomKey = password
        session.position = position

        sendOther(room, session.id, WsCode.JOIN, memberJson(room.players[position]!!))
        sendJoinResponse(session, room, position)
        onStateChanged()
    }

    private fun handleQuit(session: SessionState) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.QUIT, WsResponseCode.NOT_LOGIN)
            return
        }
        val name = session.name.orEmpty()
        val position = session.position
        if (position != null) room.players.remove(position)
        session.name = null
        session.roomKey = null
        session.position = null
        sendAll(room, WsCode.QUIT, jsonObject("name" to name))
        sendWithoutData(session, Routes.QUIT, WsResponseCode.OK)
        pruneRoom(room)
        onStateChanged()
    }

    private fun handleDisband(session: SessionState) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.DISBAND, WsResponseCode.NOT_LOGIN)
            return
        }
        if (session.position != 0) {
            sendWithoutData(session, Routes.DISBAND, WsResponseCode.NO_PERMISSION)
            return
        }
        sendOther(room, session.id, WsCode.DISBAND, jsonObject("name" to session.name.orEmpty()))
        rooms.remove(room.key)
        sessions.values.filter { it.roomKey == room.key }.forEach {
            it.roomKey = null
            it.position = null
        }
        sendWithoutData(session, Routes.DISBAND, WsResponseCode.OK)
        onStateChanged()
    }

    private fun handleSetting(session: SessionState, data: JSONObject) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.SETTING, WsResponseCode.NOT_LOGIN)
            return
        }
        if (session.position != 0) {
            sendWithoutData(session, Routes.SETTING, WsResponseCode.NO_PERMISSION)
            return
        }
        val configs = data.optJsonObject("current_configs") ?: JSONObject()
        for (key in configs.keys()) {
            val value = configs.optInt(key)
            if (value >= 0) room.configs[key] = value
        }
        val payload = jsonObject("current_configs" to room.configs)
        sendWithoutData(session, Routes.SETTING, WsResponseCode.OK)
        sendOther(room, session.id, WsCode.SETTING, payload)
    }

    private fun handleStart(session: SessionState) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.START, WsResponseCode.NOT_LOGIN)
            return
        }
        if (session.position != 0) {
            sendWithoutData(session, Routes.START, WsResponseCode.NO_PERMISSION)
            return
        }
        if (room.players.size != 3 || room.game != null) {
            sendWithoutData(session, Routes.START, WsResponseCode.NOT_IN_RANGE)
            return
        }
        sendAll(room, WsCode.START, jsonObject())
        startNewDeal(room)
        sendWithoutData(session, Routes.START, WsResponseCode.OK)
    }

    private fun handleCallLandlord(session: SessionState, data: JSONObject) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.CALL_LANDLORD, WsResponseCode.NOT_LOGIN)
            return
        }
        val game = room.game ?: run {
            sendWithoutData(session, Routes.CALL_LANDLORD, WsResponseCode.NO_PERMISSION)
            return
        }
        val pos = session.position ?: return sendWithoutData(session, Routes.CALL_LANDLORD, WsResponseCode.NOT_LOGIN)
        val score = data.optInt("score", -1)
        if (game.phase != LandlordPhase.CALL_LANDLORD || game.currentPosition != pos || score !in 0..3 || (score > 0 && score <= game.score)) {
            sendWithoutData(session, Routes.CALL_LANDLORD, WsResponseCode.NO_PERMISSION)
            return
        }
        if (score > 0) game.score = score
        game.callHistory.add(CallRecord(pos, score))
        sendAll(room, WsCode.CALL_LANDLORD, jsonObject("name" to session.name.orEmpty(), "score" to score))
        sendWithoutData(session, Routes.CALL_LANDLORD, WsResponseCode.OK)
        advanceCallPhase(room, game)
    }

    private fun handlePlay(session: SessionState, data: JSONObject) {
        val room = roomOf(session) ?: run {
            sendWithoutData(session, Routes.PLAY, WsResponseCode.NOT_LOGIN)
            return
        }
        val game = room.game ?: run {
            sendWithoutData(session, Routes.PLAY, WsResponseCode.NO_PERMISSION)
            return
        }
        val pos = session.position ?: return sendWithoutData(session, Routes.PLAY, WsResponseCode.NOT_LOGIN)
        val cards = data.optIntArray("cards")
        if (!validatePlay(game, pos, cards)) {
            sendWithoutData(session, Routes.PLAY, WsResponseCode.NO_PERMISSION)
            return
        }
        if (cards.isNotEmpty()) {
            game.lastPlayPosition = pos
            game.lastPlay = cards.toMutableList()
            val hand = game.hands[pos] ?: mutableListOf()
            cards.forEach { hand.remove(it) }
            game.hands[pos] = hand
        }
        sendAll(room, WsCode.PLAY, jsonObject("name" to session.name.orEmpty(), "cards" to cards))
        sendWithoutData(session, Routes.PLAY, WsResponseCode.OK)
        if (game.hands[pos].orEmpty().isEmpty()) finishGame(room, game, pos)
        else advancePlayTurn(room, game)
    }

    private fun handleAway(session: SessionState) {
        val room = roomOf(session) ?: return sendWithoutData(session, Routes.AWAY, WsResponseCode.NOT_LOGIN)
        val pos = session.position ?: return sendWithoutData(session, Routes.AWAY, WsResponseCode.NOT_LOGIN)
        room.players[pos]?.away = true
        sendAll(room, WsCode.AWAY, jsonObject("position" to pos))
        sendWithoutData(session, Routes.AWAY, WsResponseCode.OK)
    }

    private fun handleBack(session: SessionState) {
        val room = roomOf(session) ?: return sendWithoutData(session, Routes.BACK, WsResponseCode.NOT_LOGIN)
        val pos = session.position ?: return sendWithoutData(session, Routes.BACK, WsResponseCode.NOT_LOGIN)
        room.players[pos]?.away = false
        sendAll(room, WsCode.BACK, jsonObject("position" to pos))
        sendWithoutData(session, Routes.BACK, WsResponseCode.OK)
    }

    private fun advanceCallPhase(room: Room, game: LandlordGame) {
        val positions = room.players.keys.sorted()
        if (game.score == 3 || game.callHistory.size >= positions.size) {
            if (game.score == 0) {
                startNewDeal(room)
                return
            }
            val landlord = game.callHistory.maxBy { it.score }.position
            game.landlordPosition = landlord
            game.phase = LandlordPhase.PLAY
            game.currentPosition = landlord
            game.lastPlayPosition = landlord
            game.hands[landlord]?.addAll(game.hiddenCards)
            game.hands[landlord]?.sort()
            sendAll(room, WsCode.CHANGE_PHASE, jsonObject("phase" to game.phase, "position" to landlord))
            sendAll(room, WsCode.DEAL_OPEN_CARDS, jsonObject("name" to (room.players[landlord]?.name ?: ""), "cards" to game.hiddenCards))
            sendAll(room, WsCode.CHANGE_DEAL, jsonObject("position" to landlord))
            return
        }
        val idx = positions.indexOf(game.currentPosition).coerceAtLeast(0)
        game.currentPosition = positions[(idx + 1) % positions.size]
        sendAll(room, WsCode.CHANGE_DEAL, jsonObject("position" to game.currentPosition))
    }

    private fun advancePlayTurn(room: Room, game: LandlordGame) {
        val positions = room.players.keys.sorted()
        val idx = positions.indexOf(game.currentPosition).coerceAtLeast(0)
        val next = positions[(idx + 1) % positions.size]
        game.currentPosition = next
        if (next == game.lastPlayPosition) game.lastPlay.clear()
        sendAll(room, WsCode.CHANGE_DEAL, jsonObject("position" to next))
    }

    private fun startNewDeal(room: Room) {
        val positions = room.players.keys.sorted()
        if (positions.size != 3) return
        val first = positions.first()
        val game = LandlordGame(
            phase = LandlordPhase.CALL_LANDLORD,
            callPosition = first,
            currentPosition = first,
            lastPlayPosition = first,
        )
        val deck = (1..54).shuffled(Random(System.nanoTime()))
        positions.forEachIndexed { index, position ->
            game.hands[position] = deck.subList(index * 17, index * 17 + 17).sorted().toMutableList()
        }
        game.hiddenCards = deck.subList(51, 54).toMutableList()
        room.game = game

        positions.forEach { position ->
            val player = room.players[position] ?: return@forEach
            val target = sessionById(player.sessionId) ?: return@forEach
            sendEvent(target, WsCode.DEAL, jsonObject("name" to player.name, "cards" to game.hands[position].orEmpty()))
        }
        sendAll(room, WsCode.CHANGE_PHASE, jsonObject("phase" to game.phase, "position" to game.currentPosition))
        sendAll(room, WsCode.CHANGE_DEAL, jsonObject("position" to game.currentPosition))
    }

    private fun finishGame(room: Room, game: LandlordGame, winner: Int) {
        game.phase = LandlordPhase.SETTLEMENT
        sendAll(room, WsCode.CHANGE_PHASE, jsonObject("phase" to game.phase, "position" to game.currentPosition))
        for ((position, hand) in game.hands) {
            sendAll(room, WsCode.DEAL_OPEN_CARDS, jsonObject("name" to (room.players[position]?.name ?: ""), "cards" to hand))
        }
        sendAll(room, WsCode.SHOW_HIDDEN_CARDS, jsonObject("name" to (room.players[game.landlordPosition]?.name ?: ""), "cards" to game.hiddenCards))
        sendAll(room, WsCode.GAME_OVER, jsonObject("is_landlord" to (winner == game.landlordPosition)))
        room.game = null
    }

    private fun sendJoinResponse(session: SessionState, room: Room?, position: Int) {
        if (room == null) {
            sendWithoutData(session, Routes.JOIN, WsResponseCode.NO_PERMISSION)
            return
        }
        val members = room.players.values
            .filter { it.position != position }
            .sortedBy { it.position }
            .map { memberJson(it) }
        val payload = jsonObject(
            "current_configs" to room.configs,
            "existing_members" to members,
            "param_descriptions" to if (position == 0) paramDescriptionsJson() else JSONObject.NULL,
            "rejoin_data" to rejoinData(room, position),
        )
        sendResponse(session, Routes.JOIN, WsResponseCode.JOINED, payload)
    }

    private fun rejoinData(room: Room, position: Int): Any {
        val game = room.game ?: return JSONObject.NULL
        if (game.phase != LandlordPhase.CALL_LANDLORD && game.phase != LandlordPhase.PLAY) return JSONObject.NULL
        val otherCounts = JSONObject()
        game.hands.filterKeys { it != position }.forEach { (pos, cards) -> otherCounts.put(pos.toString(), cards.size) }
        return jsonObject(
            "other_cards_numbers" to otherCounts,
            "my_cards" to game.hands[position].orEmpty(),
            "now_playing" to game.currentPosition,
            "phase" to game.phase,
            "landlord_position" to (game.landlordPosition ?: JSONObject.NULL),
            "score" to game.score,
            "hidden_cards" to if (game.phase == LandlordPhase.PLAY) game.hiddenCards else emptyList<Int>(),
            "last_play_position" to if (game.lastPlay.isEmpty()) JSONObject.NULL else game.lastPlayPosition,
            "last_play" to game.lastPlay,
        )
    }

    private fun markDisconnected(session: SessionState) {
        val room = roomOf(session) ?: return
        val pos = session.position ?: return
        room.players[pos]?.active = false
        session.roomKey = null
        session.position = null
        sendAll(room, WsCode.JOIN, memberJson(room.players[pos]!!), excludeSessionId = session.id)
    }

    private fun firstFreePosition(room: Room): Int? = (0..2).firstOrNull { room.players[it] == null }

    private fun pruneRoom(room: Room) {
        if (room.players.isEmpty()) rooms.remove(room.key)
    }

    private fun roomOf(session: SessionState): Room? = session.roomKey?.let { rooms[it] }

    private fun sessionById(id: Long): SessionState? = sessions.values.firstOrNull { it.id == id }

    private fun sendWithoutData(session: SessionState, route: Int, code: Int) {
        session.socket.send(jsonObject("route" to route, "code" to code).toString())
    }

    private fun sendResponse(session: SessionState, route: Int, code: Int, data: JSONObject) {
        session.socket.send(jsonObject("route" to route, "code" to code, "data" to data).toString())
    }

    private fun sendEvent(session: SessionState, code: Int, data: JSONObject) {
        session.socket.send(jsonObject("code" to code, "data" to data).toString())
    }

    private fun sendAll(room: Room, code: Int, data: JSONObject, excludeSessionId: Long? = null) {
        room.players.values.forEach { player ->
            if (excludeSessionId == player.sessionId) return@forEach
            sessionById(player.sessionId)?.let { sendEvent(it, code, data) }
        }
    }

    private fun sendOther(room: Room, sessionId: Long, code: Int, data: JSONObject) {
        sendAll(room, code, data, excludeSessionId = sessionId)
    }

    private fun memberJson(player: Player): JSONObject = jsonObject(
        "name" to player.name,
        "position" to player.position,
        "avatar_url" to player.avatarUrl,
        "is_active" to player.active,
    )

    private fun paramDescriptionsJson(): JSONObject = jsonObject(
        "start_time" to jsonObject("Range" to jsonObject("default" to 1, "min" to 0, "max" to 10)),
        "settlement_time" to jsonObject("Range" to jsonObject("default" to 5, "min" to 0, "max" to 30)),
        "turn_time" to jsonObject("Range" to jsonObject("default" to 20, "min" to 5, "max" to 120)),
    )
}
