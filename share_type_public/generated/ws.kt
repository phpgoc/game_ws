@Serializable
data class CodeResponse (
	val code: Int
)

@Serializable
data class CommonResponse<T> (
	val code: T,
	val message: String
)

@Serializable
data class CreateRequestData (
	val name: String,
	val password: String
)

@Serializable
data class JoinRequestData (
	val name: String,
	val password: String
)

@Serializable
data class SwapPositionCommonData (
	val a: String,
	val b: String
)

@Serializable
data class WsMessage (
	val common: CommonResponse<Int>,
	val topic: String
)

@Serializable
data class WsRequest<T> (
	val route_code: Int,
	val data: T
)

