package com.example.landlordserver

import android.content.Intent

data class ServerStatus(
    val running: Boolean,
    val host: String,
    val port: Int,
    val clientCount: Int,
    val roomCount: Int,
) {
    val statusText: String
        get() = if (running) "运行中" else "已停止"

    fun writeTo(intent: Intent) {
        intent.putExtra(EXTRA_RUNNING, running)
        intent.putExtra(EXTRA_HOST, host)
        intent.putExtra(EXTRA_PORT, port)
        intent.putExtra(EXTRA_CLIENT_COUNT, clientCount)
        intent.putExtra(EXTRA_ROOM_COUNT, roomCount)
    }

    companion object {
        private const val EXTRA_RUNNING = "running"
        private const val EXTRA_HOST = "host"
        private const val EXTRA_PORT = "port"
        private const val EXTRA_CLIENT_COUNT = "client_count"
        private const val EXTRA_ROOM_COUNT = "room_count"

        fun fromIntent(intent: Intent): ServerStatus = ServerStatus(
            running = intent.getBooleanExtra(EXTRA_RUNNING, false),
            host = intent.getStringExtra(EXTRA_HOST) ?: "0.0.0.0",
            port = intent.getIntExtra(EXTRA_PORT, 9001),
            clientCount = intent.getIntExtra(EXTRA_CLIENT_COUNT, 0),
            roomCount = intent.getIntExtra(EXTRA_ROOM_COUNT, 0),
        )
    }
}
