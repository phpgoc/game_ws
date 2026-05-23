# share_type_public

公共类型 crate（可开源），包含：

- `src/common.rs`
- `src/ws.rs`

API 侧也依赖这里的公共类型。

## TypeScript 生成（common + ws）

```bash
mkdir -p generated

tmp_public="$(mktemp -d)"
cp src/common.rs src/ws.rs "$tmp_public"/

typeshare "$tmp_public" --lang kotlin --output-file ./generated/ws.kt

rm -rf "$tmp_public"
```
