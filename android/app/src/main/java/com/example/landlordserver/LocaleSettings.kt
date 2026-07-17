package com.example.langameserver

import android.content.Context
import android.content.res.Configuration
import java.util.Locale

const val LANGUAGE_ZH = "zh"
const val LANGUAGE_EN = "en"

fun selectedLanguage(context: Context): String {
    val saved = context
        .getSharedPreferences(LOCALE_PREFS_NAME, Context.MODE_PRIVATE)
        .getString(KEY_SELECTED_LANGUAGE, null)
    return when (saved) {
        LANGUAGE_EN -> LANGUAGE_EN
        LANGUAGE_ZH -> LANGUAGE_ZH
        else -> LANGUAGE_ZH
    }
}

fun saveSelectedLanguage(context: Context, language: String) {
    val normalized = when (language) {
        LANGUAGE_EN -> LANGUAGE_EN
        else -> LANGUAGE_ZH
    }
    context
        .getSharedPreferences(LOCALE_PREFS_NAME, Context.MODE_PRIVATE)
        .edit()
        .putString(KEY_SELECTED_LANGUAGE, normalized)
        .apply()
}

fun localizedContext(context: Context): Context {
    val locale = when (selectedLanguage(context)) {
        LANGUAGE_EN -> Locale.ENGLISH
        else -> Locale.SIMPLIFIED_CHINESE
    }
    Locale.setDefault(locale)
    val config = Configuration(context.resources.configuration)
    config.setLocale(locale)
    config.setLayoutDirection(locale)
    return context.createConfigurationContext(config)
}

private val LOCALE_PREFS_NAME = "${BuildConfig.GAME_ID}_server"
private const val KEY_SELECTED_LANGUAGE = "selected_language"
