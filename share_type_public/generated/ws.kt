@Serializable
enum class WsCode(val string: String) {
	@SerialName("JOIN")
	JOIN("JOIN"),
	@SerialName("QUIT")
	QUIT("QUIT"),
	@SerialName("MESSAGE")
	MESSAGE("MESSAGE"),
	@SerialName("PAUSE")
	PAUSE("PAUSE"),
	@SerialName("RESUME")
	RESUME("RESUME"),
	@SerialName("DISBAND")
	DISBAND("DISBAND"),
	@SerialName("SETTING")
	SETTING("SETTING"),
	@SerialName("DEAL")
	DEAL("DEAL"),
	@SerialName("PLAY")
	PLAY("PLAY"),
	@SerialName("AWAY")
	AWAY("AWAY"),
}

@Serializable
data class CommonResponse<T> (
	val code: WsCode,
	val data: T
)

@Serializable
data class CommonWithoutDataResponse (
	val code: WsCode
)

@Serializable
data class SwapPositionPayload (
	val a: String,
	val b: String
)

@Serializable
data class WsCreateRequest (
	val name: String,
	val password: String
)

@Serializable
data class WsJoinRequest (
	val name: String,
	val password: String
)

@Serializable
data class WsJoinResponse<T> (
	val name: String,
	val settings: T
)

@Serializable
data class WsMessageRequest (
	val message: String
)

@Serializable
data class WsMessageResponse (
	val name: String,
	val message: String
)

@Serializable
enum class Routes(val string: String) {
	@SerialName("CREATE")
	CREATE("CREATE"),
	@SerialName("JOIN")
	JOIN("JOIN"),
	@SerialName("QUIT")
	QUIT("QUIT"),
	@SerialName("MESSAGE")
	MESSAGE("MESSAGE"),
	@SerialName("PAUSE")
	PAUSE("PAUSE"),
	@SerialName("RESUME")
	RESUME("RESUME"),
	@SerialName("DISBAND")
	DISBAND("DISBAND"),
	@SerialName("SETTING")
	SETTING("SETTING"),
	@SerialName("DEAL")
	DEAL("DEAL"),
	@SerialName("PLAY")
	PLAY("PLAY"),
	@SerialName("AWAY")
	AWAY("AWAY"),
}

@Serializable
data class WsRequest<T> (
	val code: Routes,
	val data: T
)

@Serializable
data class WsWithoutDataRequest (
	val code: Routes
)

