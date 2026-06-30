package com.example.landlordserver.server

object GameId {
    const val LANDLORD = 1
}

object Routes {
    const val JOIN = 2
    const val QUIT = 3
    const val MESSAGE = 4
    const val PAUSE = 5
    const val RESUME = 6
    const val DISBAND = 7
    const val SETTING = 8
    const val START = 10
    const val AWAY = 12
    const val BACK = 13
    const val SWAP = 14
    const val PLAY = 21
    const val CALL_LANDLORD = 1001
}

object WsCode {
    const val JOIN = 2
    const val QUIT = 3
    const val MESSAGE = 4
    const val DISBAND = 7
    const val SETTING = 8
    const val START = 10
    const val GAME_OVER = 11
    const val AWAY = 12
    const val BACK = 13
    const val SWAP = 14
    const val DEAL = 20
    const val PLAY = 21
    const val DEAL_OPEN_CARDS = 24
    const val CHANGE_DEAL = 26
    const val CHANGE_PHASE = 27
    const val SHOW_HIDDEN_CARDS = 30
    const val CALL_LANDLORD = 1001
}

object WsResponseCode {
    const val OK = 0
    const val JOINED = 201
    const val ERROR_FORMAT = 400
    const val NOT_LOGIN = 401
    const val WRONG_GAME = 402
    const val NO_PERMISSION = 403
    const val NOT_IN_RANGE = 410
}

object LandlordPhase {
    const val START = 0
    const val CALL_LANDLORD = 1
    const val PLAY = 2
    const val SETTLEMENT = 3
}
