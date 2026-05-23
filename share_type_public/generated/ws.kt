@Serializable
data class CommonMessage (
	val code: Int,
	val message: String
)

@Serializable
data class WsMessage (
	val common: CommonMessage,
	val topic: String
)

