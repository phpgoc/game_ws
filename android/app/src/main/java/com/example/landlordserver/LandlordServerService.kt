package com.example.langameserver

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import com.example.langameserver.rust.NativeServer

class LandlordServerService : Service() {
    @Volatile
    private var running = false

    private val mainHandler = Handler(Looper.getMainLooper())
    private val statusTicker = object : Runnable {
        override fun run() {
            if (!running) return
            broadcastStatus()
            updateNotification()
            mainHandler.postDelayed(this, STATUS_REFRESH_MS)
        }
    }
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private val port = ActiveGameServer.port

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(localizedContext(newBase))
    }

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
            ACTION_STATUS_REQUEST -> {
                broadcastStatus()
                if (running) updateNotification()
            }
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
        if (running) {
            broadcastStatus()
            return
        }

        acquireLocks()
        val host = selectedIpv4Address(this)
        val started = runCatching { NativeServer.start(port) }.getOrDefault(false)
        if (!started) {
            releaseLocks()
            running = false
            broadcastStatus()
            stopSelf()
            return
        }
        running = true
        startForeground(NOTIFICATION_ID, buildNotification(host))
        mainHandler.removeCallbacks(statusTicker)
        mainHandler.post(statusTicker)
        broadcastStatus()
    }

    private fun stopServer() {
        mainHandler.removeCallbacks(statusTicker)
        if (running) {
            NativeServer.stop()
        }
        running = false
        releaseLocks()
        broadcastStatus()
    }

    private fun acquireLocks() {
        val pm = getSystemService(PowerManager::class.java)
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "LanGameWsServer:Wake").apply {
            setReferenceCounted(false)
            acquire()
        }
        val wm = applicationContext.getSystemService(WifiManager::class.java)
        wifiLock = wm.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "LanGameWsServer:Wifi").apply {
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
        running = running,
        host = selectedIpv4Address(this),
        port = port,
        clientCount = if (running) NativeServer.clientCount() else 0,
        roomCount = if (running) NativeServer.roomCount() else 0,
    )

    private fun updateNotification() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildNotification(selectedIpv4Address(this)))
    }

    private fun buildNotification(host: String): Notification {
        val locale = localizedContext(this)
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setContentTitle(locale.getString(R.string.notification_title_running))
            .setContentText(
                locale.getString(
                    R.string.notification_text_format,
                    host,
                    port,
                    if (running) NativeServer.clientCount() else 0,
                ),
            )
            .setOngoing(true)
            .build()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val locale = localizedContext(this)
        val channel = NotificationChannel(
            CHANNEL_ID,
            locale.getString(R.string.notification_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        val ACTION_STATUS = "${BuildConfig.APPLICATION_ID}.STATUS"
        private val ACTION_STOP = "${BuildConfig.APPLICATION_ID}.STOP"
        private val ACTION_STATUS_REQUEST = "${BuildConfig.APPLICATION_ID}.STATUS_REQUEST"
        private val CHANNEL_ID = "${ActiveGameServer.id}_ws_server"
        private val NOTIFICATION_ID = BuildConfig.SERVER_PORT
        private const val STATUS_REFRESH_MS = 2_000L

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
