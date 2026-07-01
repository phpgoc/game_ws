package com.example.landlordserver

import java.net.Inet4Address
import java.net.NetworkInterface

//fun localIpv4Address(): String {
//    return NetworkInterface.getNetworkInterfaces().asSequence()
//        .flatMap { it.inetAddresses.asSequence() }
//        .firstOrNull { !it.isLoopbackAddress && it.hostAddress?.contains(':') == false }
//        ?.hostAddress
//        ?: "127.0.0.1"
//}

fun localIpv4Address(): String {
    return runCatching {
        NetworkInterface.getNetworkInterfaces()
            .asSequence()
            .filter { it.isUp && !it.isLoopback }
            .flatMap { it.inetAddresses.asSequence() }
            .filterIsInstance<Inet4Address>()
            .mapNotNull { it.hostAddress }
            // 优先匹配常见局域网网段，192 > 172 > 10
            .sortedBy {
                when {
                    it.startsWith("192.168.") -> 0
                    it.startsWith("172.") -> 1
                    it.startsWith("10.") -> 2
                    else -> 3
                }
            }
            .firstOrNull()
    }.getOrNull() ?: "127.0.0.1"
}