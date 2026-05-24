# share_type_public

公共类型 crate（可开源），包含：

- `src/common.rs`
- `src/const.rs`（路由码常量）
- `src/ws.rs`

API 侧也依赖这里的公共类型。

其中 `src/common.rs` 里有：

- `CommonRequest<T> { code: Routes, data }`
- `CommonResponse<T> { code: WsCode, data }`

## 修改后重生成

在 `share_type_public` 目录执行：

```bash
mkdir -p generated

tmp_public="$(mktemp -d)"
cp src/common.rs src/ws.rs src/const.rs "$tmp_public"/

# 结构体类型（Kotlin）
typeshare "$tmp_public" --lang kotlin --output-file ./generated/ws.kt

rm -rf "$tmp_public"
```

生成结果：

- `generated/ws.kt`
