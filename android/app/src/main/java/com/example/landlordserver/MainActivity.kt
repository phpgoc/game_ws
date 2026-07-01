package com.example.landlordserver

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
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
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView

class MainActivity : android.app.Activity() {
    private lateinit var statusText: TextView
    private lateinit var endpointText: TextView
    private lateinit var clientsText: TextView
    private lateinit var roomsText: TextView
    private lateinit var languageSpinner: Spinner
    private lateinit var ipSpinner: Spinner
    private var updatingLanguageSpinner = false
    private var updatingIpSpinner = false

    private val stateReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            renderState(ServerStatus.fromIntent(intent))
        }
    }

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(localizedContext(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestNotificationPermission()

        statusText = statusPill(getString(R.string.status_starting), COLOR_WARNING_BG, COLOR_WARNING_TEXT)
        endpointText = endpointLabel(getString(R.string.endpoint_empty))
        clientsText = metricLabel(getString(R.string.client_count_format, 0))
        roomsText = metricLabel(getString(R.string.room_count_format, 0))
        languageSpinner = Spinner(this).apply {
            background = fieldBackground()
            minimumHeight = dp(48)
            layoutParams = blockLayoutParams()
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onItemSelected(
                    parent: AdapterView<*>?,
                    view: View?,
                    position: Int,
                    id: Long,
                ) {
                    if (updatingLanguageSpinner) return
                    val language = if (position == 1) LANGUAGE_EN else LANGUAGE_ZH
                    if (language == selectedLanguage(this@MainActivity)) return
                    saveSelectedLanguage(this@MainActivity, language)
                    LandlordServerService.requestStatus(this@MainActivity)
                    recreate()
                }

                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            }
        }
        ipSpinner = Spinner(this).apply {
            background = fieldBackground()
            minimumHeight = dp(48)
            layoutParams = blockLayoutParams()
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onItemSelected(
                    parent: AdapterView<*>?,
                    view: View?,
                    position: Int,
                    id: Long,
                ) {
                    if (updatingIpSpinner) return
                    val host = privateIpv4AddressFromLabel(
                        this@MainActivity,
                        parent?.getItemAtPosition(position)?.toString().orEmpty(),
                    )
                    saveSelectedIpv4Address(this@MainActivity, host)
                    endpointText.text = getString(R.string.endpoint_format, host, SERVER_PORT)
                    LandlordServerService.requestStatus(this@MainActivity)
                }

                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            }
        }

        val startButton = actionButton(getString(R.string.start_service), filled = true).apply {
            setOnClickListener {
                LandlordServerService.start(this@MainActivity)
            }
        }
        val stopButton = actionButton(getString(R.string.stop_service), filled = false).apply {
            setOnClickListener {
                LandlordServerService.stop(this@MainActivity)
            }
        }
        val batteryButton = actionButton(getString(R.string.battery_optimization_settings), filled = false).apply {
            setOnClickListener { openBatterySettings() }
        }

        val actions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            layoutParams = blockLayoutParams(top = 12)
            addView(startButton, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply {
                marginEnd = dp(8)
            })
            addView(stopButton, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply {
                marginStart = dp(8)
            })
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(28), dp(20), dp(28))
            addView(headerCard())
            addView(section(getString(R.string.service_status_title), listOf(statusText, actions)))
            addView(
                section(
                    getString(R.string.network_section_title),
                    listOf(
                        fieldLabel(getString(R.string.private_ip_label)),
                        ipSpinner,
                        endpointText,
                        metricRow(),
                    ),
                ),
            )
            addView(section(getString(R.string.settings_section_title), listOf(
                fieldLabel(getString(R.string.language_label)),
                languageSpinner,
                batteryButton,
            )))
        }

        setContentView(ScrollView(this).apply {
            setBackgroundColor(COLOR_PAGE_BG)
            isFillViewport = true
            addView(root)
        })
        refreshLanguageList()
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
        refreshLanguageList()
        refreshIpList()
        LandlordServerService.requestStatus(this)
    }

    override fun onStop() {
        unregisterReceiver(stateReceiver)
        super.onStop()
    }

    private fun renderState(state: ServerStatus) {
        val status = if (state.running) R.string.status_running else R.string.status_stopped
        statusText.text = getString(R.string.status_format, getString(status))
        statusText.background = pillBackground(
            if (state.running) COLOR_SUCCESS_BG else COLOR_DANGER_BG,
        )
        statusText.setTextColor(if (state.running) COLOR_SUCCESS_TEXT else COLOR_DANGER_TEXT)
        endpointText.text = getString(R.string.endpoint_format, state.host, state.port)
        clientsText.text = getString(R.string.client_count_format, state.clientCount)
        roomsText.text = getString(R.string.room_count_format, state.roomCount)
    }

    private fun refreshLanguageList() {
        val items = listOf(
            getString(R.string.language_zh),
            getString(R.string.language_en),
        )
        val selectedIndex = if (selectedLanguage(this) == LANGUAGE_EN) 1 else 0
        updatingLanguageSpinner = true
        languageSpinner.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_item, items).apply {
            setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        }
        languageSpinner.setSelection(selectedIndex, false)
        updatingLanguageSpinner = false
    }

    private fun refreshIpList() {
        val addresses = privateIpv4AddressEntries()
        val items = if (addresses.isEmpty()) {
            listOf(getString(R.string.private_ipv4_not_found))
        } else {
            addresses.map { it.toString() }
        }
        val selected = selectedIpv4Address(this)
        val selectedLabel = privateIpv4AddressLabel(this, selected)
        updatingIpSpinner = true
        ipSpinner.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_item, items).apply {
            setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        }
        val selectedIndex = items.indexOf(selectedLabel).takeIf { it >= 0 } ?: 0
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

    private fun headerCard() = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = roundedBackground(COLOR_HEADER_BG, radius = 24f)
        setPadding(dp(22), dp(22), dp(22), dp(22))
        addView(TextView(this@MainActivity).apply {
            text = getString(ActiveGameServer.serviceTitleRes)
            textSize = 26f
            typeface = Typeface.DEFAULT_BOLD
            setTextColor(Color.WHITE)
        })
        addView(TextView(this@MainActivity).apply {
            text = getString(
                R.string.server_subtitle_format,
                getString(ActiveGameServer.gameNameRes),
            )
            textSize = 15f
            setTextColor(0xDDEAF2FF.toInt())
            setPadding(0, dp(8), 0, 0)
        })
        layoutParams = cardLayoutParams()
    }

    private fun section(title: String, children: List<View>) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = roundedBackground(Color.WHITE, strokeColor = COLOR_BORDER, radius = 18f)
        setPadding(dp(18), dp(16), dp(18), dp(18))
        addView(TextView(this@MainActivity).apply {
            text = title
            textSize = 16f
            typeface = Typeface.DEFAULT_BOLD
            setTextColor(COLOR_TEXT)
            setPadding(0, 0, 0, dp(12))
        })
        children.forEach { child ->
            addView(child)
        }
        layoutParams = cardLayoutParams()
    }

    private fun metricRow() = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER
        setPadding(0, dp(10), 0, 0)
        addView(clientsText, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply {
            marginEnd = dp(8)
        })
        addView(roomsText, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply {
            marginStart = dp(8)
        })
    }

    private fun statusPill(text: String, bgColor: Int, textColor: Int) = TextView(this).apply {
        this.text = text
        textSize = 18f
        typeface = Typeface.DEFAULT_BOLD
        gravity = Gravity.CENTER
        setTextColor(textColor)
        background = pillBackground(bgColor)
        setPadding(dp(18), dp(12), dp(18), dp(12))
        layoutParams = blockLayoutParams()
    }

    private fun endpointLabel(text: String) = TextView(this).apply {
        this.text = text
        textSize = 16f
        setTextColor(COLOR_TEXT)
        background = roundedBackground(COLOR_FIELD_BG, strokeColor = COLOR_BORDER, radius = 12f)
        setPadding(dp(14), dp(12), dp(14), dp(12))
        layoutParams = blockLayoutParams(top = 12)
    }

    private fun metricLabel(text: String) = TextView(this).apply {
        this.text = text
        textSize = 16f
        gravity = Gravity.CENTER
        setTextColor(COLOR_TEXT)
        background = roundedBackground(COLOR_FIELD_BG, strokeColor = COLOR_BORDER, radius = 12f)
        setPadding(dp(12), dp(12), dp(12), dp(12))
    }

    private fun fieldLabel(text: String) = TextView(this).apply {
        this.text = text
        textSize = 14f
        typeface = Typeface.DEFAULT_BOLD
        setTextColor(COLOR_MUTED_TEXT)
        setPadding(0, dp(8), 0, dp(6))
    }

    private fun actionButton(text: String, filled: Boolean) = Button(this).apply {
        this.text = text
        isAllCaps = false
        textSize = 14f
        minHeight = dp(48)
        setTextColor(if (filled) Color.WHITE else COLOR_ACCENT)
        background = if (filled) {
            roundedBackground(COLOR_ACCENT, radius = 12f)
        } else {
            roundedBackground(Color.TRANSPARENT, strokeColor = COLOR_ACCENT, radius = 12f)
        }
        setPadding(dp(12), 0, dp(12), 0)
        layoutParams = blockLayoutParams(top = 12)
    }

    private fun fieldBackground() = roundedBackground(COLOR_FIELD_BG, strokeColor = COLOR_BORDER, radius = 12f)

    private fun roundedBackground(
        color: Int,
        strokeColor: Int? = null,
        radius: Float,
    ) = GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        setColor(color)
        cornerRadius = dp(radius.toInt()).toFloat()
        if (strokeColor != null) setStroke(dp(1), strokeColor)
    }

    private fun pillBackground(color: Int) = roundedBackground(color, radius = 999f)

    private fun cardLayoutParams() = LinearLayout.LayoutParams(
        LinearLayout.LayoutParams.MATCH_PARENT,
        LinearLayout.LayoutParams.WRAP_CONTENT,
    ).apply {
        bottomMargin = dp(14)
    }

    private fun blockLayoutParams(top: Int = 0) = LinearLayout.LayoutParams(
        LinearLayout.LayoutParams.MATCH_PARENT,
        LinearLayout.LayoutParams.WRAP_CONTENT,
    ).apply {
        topMargin = dp(top)
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private companion object {
        val SERVER_PORT = ActiveGameServer.port
        const val COLOR_PAGE_BG = 0xFFF5F7FB.toInt()
        const val COLOR_HEADER_BG = 0xFF1F4E79.toInt()
        const val COLOR_ACCENT = 0xFF2563EB.toInt()
        const val COLOR_TEXT = 0xFF172033.toInt()
        const val COLOR_MUTED_TEXT = 0xFF5B667A.toInt()
        const val COLOR_BORDER = 0xFFD8DEE9.toInt()
        const val COLOR_FIELD_BG = 0xFFF8FAFD.toInt()
        const val COLOR_SUCCESS_BG = 0xFFDFF7EA.toInt()
        const val COLOR_SUCCESS_TEXT = 0xFF137A3D.toInt()
        const val COLOR_WARNING_BG = 0xFFFFF4D6.toInt()
        const val COLOR_WARNING_TEXT = 0xFF8A5A00.toInt()
        const val COLOR_DANGER_BG = 0xFFFFE4E6.toInt()
        const val COLOR_DANGER_TEXT = 0xFFB42318.toInt()
    }
}
