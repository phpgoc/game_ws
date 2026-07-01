package com.example.landlordserver

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.Spinner
import android.widget.TextView

class MainActivity : android.app.Activity() {
    private lateinit var statusText: TextView
    private lateinit var endpointText: TextView
    private lateinit var clientsText: TextView
    private lateinit var roomsText: TextView
    private lateinit var ipSpinner: Spinner
    private var updatingIpSpinner = false

    private val stateReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            renderState(ServerStatus.fromIntent(intent))
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestNotificationPermission()

        statusText = label("状态: 启动中")
        endpointText = label("地址: -")
        clientsText = label("连接数: 0")
        roomsText = label("房间数: 0")
        ipSpinner = Spinner(this).apply {
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onItemSelected(
                    parent: AdapterView<*>?,
                    view: View?,
                    position: Int,
                    id: Long,
                ) {
                    if (updatingIpSpinner) return
                    val host = parent?.getItemAtPosition(position)?.toString().orEmpty()
                    saveSelectedIpv4Address(this@MainActivity, host)
                    endpointText.text = "地址: ws://$host:9001"
                    LandlordServerService.requestStatus(this@MainActivity)
                }

                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            }
        }

        val startButton = Button(this).apply {
            text = "启动服务"
            setOnClickListener {
                LandlordServerService.start(this@MainActivity)
            }
        }
        val stopButton = Button(this).apply {
            text = "停止服务"
            setOnClickListener {
                LandlordServerService.stop(this@MainActivity)
            }
        }
        val batteryButton = Button(this).apply {
            text = "电池优化设置"
            setOnClickListener { openBatterySettings() }
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(40, 64, 40, 40)
            addView(title("斗地主 WS 服务"))
            addView(statusText)
            addView(label("内网 IP"))
            addView(ipSpinner)
            addView(endpointText)
            addView(clientsText)
            addView(roomsText)
            addView(startButton)
            addView(stopButton)
            addView(batteryButton)
        }

        setContentView(root)
        LandlordServerService.start(this)
    }

    override fun onStart() {
        super.onStart()
        val filter = IntentFilter(LandlordServerService.ACTION_STATUS)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(stateReceiver, filter, RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("DEPRECATION")
            registerReceiver(stateReceiver, filter)
        }
        refreshIpList()
        LandlordServerService.requestStatus(this)
    }

    override fun onStop() {
        unregisterReceiver(stateReceiver)
        super.onStop()
    }

    private fun renderState(state: ServerStatus) {
        statusText.text = "状态: ${state.statusText}"
        endpointText.text = "地址: ws://${state.host}:${state.port}"
        clientsText.text = "连接数: ${state.clientCount}"
        roomsText.text = "房间数: ${state.roomCount}"
    }

    private fun refreshIpList() {
        val addresses = privateIpv4Addresses()
        val items = if (addresses.isEmpty()) listOf("未找到 private IPv4") else addresses
        val selected = selectedIpv4Address(this)
        updatingIpSpinner = true
        ipSpinner.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_item, items).apply {
            setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        }
        val selectedIndex = items.indexOf(selected).takeIf { it >= 0 } ?: 0
        ipSpinner.setSelection(selectedIndex, false)
        ipSpinner.isEnabled = addresses.isNotEmpty()
        updatingIpSpinner = false
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 100)
    }

    private fun openBatterySettings() {
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
            data = Uri.parse("package:$packageName")
        }
        runCatching { startActivity(intent) }
            .onFailure { startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
    }

    private fun title(text: String) = TextView(this).apply {
        this.text = text
        textSize = 24f
        setPadding(0, 0, 0, 32)
    }

    private fun label(text: String) = TextView(this).apply {
        this.text = text
        textSize = 18f
        setPadding(0, 12, 0, 12)
    }
}
