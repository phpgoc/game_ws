# share_type_public

- 用于websocket服务器和客户端之间的通信。


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
