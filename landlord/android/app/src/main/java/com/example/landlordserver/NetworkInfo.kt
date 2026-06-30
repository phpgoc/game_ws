package com.example.landlordserver

import java.net.NetworkInterface

fun localIpv4Address(): String {
    return NetworkInterface.getNetworkInterfaces().asSequence()
        .flatMap { it.inetAddresses.asSequence() }
        .firstOrNull { !it.isLoopbackAddress && it.hostAddress?.contains(':') == false }
        ?.hostAddress
        ?: "127.0.0.1"
}
