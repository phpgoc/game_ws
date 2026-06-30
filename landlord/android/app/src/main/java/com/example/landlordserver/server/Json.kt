package com.example.landlordserver.server

import org.json.JSONArray
import org.json.JSONObject

fun jsonObject(vararg pairs: Pair<String, Any?>): JSONObject {
    val obj = JSONObject()
    for ((key, value) in pairs) obj.put(key, toJsonValue(value))
    return obj
}

fun jsonArray(values: Iterable<Any?>): JSONArray {
    val arr = JSONArray()
    for (value in values) arr.put(toJsonValue(value))
    return arr
}

fun JSONObject.optIntOrNull(key: String): Int? =
    if (has(key) && !isNull(key)) optInt(key) else null

fun JSONObject.optStringOrEmpty(key: String): String =
    if (has(key) && !isNull(key)) optString(key) else ""

fun JSONObject.optJsonObject(key: String): JSONObject? =
    if (has(key) && !isNull(key)) optJSONObject(key) else null

fun JSONObject.optIntArray(key: String): MutableList<Int> {
    val arr = optJSONArray(key) ?: return mutableListOf()
    val out = mutableListOf<Int>()
    for (i in 0 until arr.length()) out.add(arr.optInt(i))
    return out
}

private fun toJsonValue(value: Any?): Any? {
    return when (value) {
        null -> JSONObject.NULL
        is JSONObject -> value
        is JSONArray -> value
        is Map<*, *> -> {
            val obj = JSONObject()
            for ((mapKey, mapValue) in value) {
                obj.put(mapKey.toString(), toJsonValue(mapValue))
            }
            obj
        }
        is Iterable<*> -> jsonArray(value)
        else -> value
    }
}
