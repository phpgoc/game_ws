package com.example.landlordserver

import android.content.Context
import java.net.Inet4Address
import java.net.NetworkInterface

data class PrivateIpv4Address(
    val address: String,
    val interfaceName: String,
) {
    override fun toString(): String = "$address ($interfaceName)"
}

fun localIpv4Address(): String {
    return privateIpv4Addresses().firstOrNull() ?: "127.0.0.1"
}

fun privateIpv4Addresses(): List<String> {
    return privateIpv4AddressEntries().map { it.address }
}

fun privateIpv4AddressEntries(): List<PrivateIpv4Address> {
    return runCatching {
        NetworkInterface.getNetworkInterfaces()
            .asSequence()
            .filter { it.isUp && !it.isLoopback }
            .flatMap { networkInterface ->
                networkInterface.inetAddresses
                    .asSequence()
                    .filterIsInstance<Inet4Address>()
                    .mapNotNull { address ->
                        val host = address.hostAddress ?: return@mapNotNull null
                        if (!isPrivateIpv4Address(host)) return@mapNotNull null
                        PrivateIpv4Address(host, networkInterface.name)
                    }
            }
            // 优先匹配常见局域网网段，192 > 172 > 10；同一地址只保留第一块网卡。
            .sortedWith(
                compareBy<PrivateIpv4Address> {
                    when {
                        it.address.startsWith("192.168.") -> 0
                        it.address.startsWith("172.") -> 1
                        it.address.startsWith("10.") -> 2
                        else -> 3
                    }
                }.thenBy { it.address }
                    .thenBy { it.interfaceName },
            )
            .distinctBy { it.address }
            .toList()
    }.getOrDefault(emptyList())
}

fun privateIpv4AddressLabel(context: Context, host: String): String {
    return privateIpv4AddressEntries()
        .firstOrNull { it.address == host }
        ?.toString()
        ?: host
}

fun privateIpv4AddressFromLabel(context: Context, label: String): String {
    return privateIpv4AddressEntries()
        .firstOrNull {
            it.address == label || it.toString() == label
        }
        ?.address
        ?: label.substringBefore(" (")
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
