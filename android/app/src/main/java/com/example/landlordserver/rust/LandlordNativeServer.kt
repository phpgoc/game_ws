package com.example.landlordserver.rust

object LandlordNativeServer {
    init {
        System.loadLibrary("landlord")
    }

    @Synchronized
    fun start(port: Int): Boolean = nativeStart(port)

    @Synchronized
    fun stop() {
        nativeStop()
    }

    fun clientCount(): Int = nativeClientCount()

    fun roomCount(): Int = nativeRoomCount()

    @JvmStatic
    private external fun nativeStart(port: Int): Boolean

    @JvmStatic
    private external fun nativeStop()

    @JvmStatic
    private external fun nativeClientCount(): Int

    @JvmStatic
    private external fun nativeRoomCount(): Int
}
