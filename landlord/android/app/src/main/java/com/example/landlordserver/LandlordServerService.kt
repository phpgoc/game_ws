package com.example.landlordserver

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import com.example.landlordserver.server.LandlordWebSocketServer
import java.net.InetSocketAddress

class LandlordServerService : Service() {
    private var server: LandlordWebSocketServer? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private val port = 9001

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopServer()
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_STATUS_REQUEST -> broadcastStatus()
            else -> startServer()
        }
        return START_STICKY
    }

    override fun onDestroy() {
        stopServer()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun startServer() {
        if (server != null) {
            broadcastStatus()
            return
        }

        acquireLocks()
        val host = localIpv4Address()
        val nextServer = LandlordWebSocketServer(InetSocketAddress("0.0.0.0", port)) {
            broadcastStatus()
            updateNotification()
        }
        nextServer.start()
        server = nextServer
        startForeground(NOTIFICATION_ID, buildNotification(host))
        broadcastStatus()
    }

    private fun stopServer() {
        server?.stop(1000)
        server = null
        releaseLocks()
        broadcastStatus()
    }

    private fun acquireLocks() {
        val pm = getSystemService(PowerManager::class.java)
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "LandlordWsServer:Wake").apply {
            setReferenceCounted(false)
            acquire()
        }
        val wm = applicationContext.getSystemService(WifiManager::class.java)
        wifiLock = wm.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "LandlordWsServer:Wifi").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun releaseLocks() {
        runCatching { wakeLock?.takeIf { it.isHeld }?.release() }
        runCatching { wifiLock?.takeIf { it.isHeld }?.release() }
        wakeLock = null
        wifiLock = null
    }

    private fun broadcastStatus() {
        val current = currentStatus()
        val intent = Intent(ACTION_STATUS)
        intent.setPackage(packageName)
        current.writeTo(intent)
        sendBroadcast(intent)
    }

    private fun currentStatus(): ServerStatus = ServerStatus(
        running = server != null,
        host = localIpv4Address(),
        port = port,
        clientCount = server?.clientCount() ?: 0,
        roomCount = server?.roomCount() ?: 0,
    )

    private fun updateNotification() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildNotification(localIpv4Address()))
    }

    private fun buildNotification(host: String): Notification {
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setContentTitle("斗地主 WS 服务运行中")
            .setContentText("ws://$host:$port · ${server?.clientCount() ?: 0} 个连接")
            .setOngoing(true)
            .build()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(CHANNEL_ID, "Landlord WS Server", NotificationManager.IMPORTANCE_LOW)
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        const val ACTION_STATUS = "com.example.landlordserver.STATUS"
        private const val ACTION_STOP = "com.example.landlordserver.STOP"
        private const val ACTION_STATUS_REQUEST = "com.example.landlordserver.STATUS_REQUEST"
        private const val CHANNEL_ID = "landlord_ws_server"
        private const val NOTIFICATION_ID = 9001

        fun start(context: Context) {
            val intent = Intent(context, LandlordServerService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(intent)
            else context.startService(intent)
        }

        fun stop(context: Context) {
            context.startService(Intent(context, LandlordServerService::class.java).setAction(ACTION_STOP))
        }

        fun requestStatus(context: Context) {
            context.startService(Intent(context, LandlordServerService::class.java).setAction(ACTION_STATUS_REQUEST))
        }
    }
}
