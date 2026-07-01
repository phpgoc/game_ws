package com.example.landlordserver

import android.content.Context
import java.net.Inet4Address
import java.net.NetworkInterface

fun localIpv4Address(): String {
    return privateIpv4Addresses().firstOrNull() ?: "127.0.0.1"
}

fun privateIpv4Addresses(): List<String> {
    return runCatching {
        NetworkInterface.getNetworkInterfaces()
            .asSequence()
            .filter { it.isUp && !it.isLoopback }
            .flatMap { it.inetAddresses.asSequence() }
            .filterIsInstance<Inet4Address>()
            .mapNotNull { it.hostAddress }
            .filter { isPrivateIpv4Address(it) }
            // 优先匹配常见局域网网段，192 > 172 > 10
            .sortedBy {
                when {
                    it.startsWith("192.168.") -> 0
                    it.startsWith("172.") -> 1
                    it.startsWith("10.") -> 2
                    else -> 3
                }
            }
            .distinct()
            .toList()
    }.getOrDefault(emptyList())
}

fun selectedIpv4Address(context: Context): String {
    val addresses = privateIpv4Addresses()
    val saved = context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .getString(KEY_SELECTED_IPV4, null)
    if (saved != null && saved in addresses) return saved
    return addresses.firstOrNull() ?: "127.0.0.1"
}

fun saveSelectedIpv4Address(context: Context, host: String) {
    if (!isPrivateIpv4Address(host)) return
    context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .edit()
        .putString(KEY_SELECTED_IPV4, host)
        .apply()
}

private fun isPrivateIpv4Address(host: String): Boolean {
    val parts = host.split(".").mapNotNull { it.toIntOrNull() }
    if (parts.size != 4 || parts.any { it !in 0..255 }) return false
    return parts[0] == 10 ||
        (parts[0] == 172 && parts[1] in 16..31) ||
        (parts[0] == 192 && parts[1] == 168)
}

private const val PREFS_NAME = "landlord_server"
private const val KEY_SELECTED_IPV4 = "selected_ipv4"
