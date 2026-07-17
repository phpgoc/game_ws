package com.example.langameserver.rust

import com.example.langameserver.BuildConfig

object NativeServer {
    init {
        System.loadLibrary(BuildConfig.RUST_LIBRARY)
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
