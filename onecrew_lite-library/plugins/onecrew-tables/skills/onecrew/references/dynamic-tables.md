# 动态表格

## 建表时先选业务类型

列定义为 `{ "name": "列名", "data_type": "类型" }`。列名称由任务决定，值结构由固定类型决定。不要传 `type`、`presentation` 或自造类型。

除非用户有特殊要求，否则不得自行添加抓取日期、数据来源、备注等辅助列；只创建用户要求及完成任务必需的业务列，来源和检索时间需要说明时放在最终回复中。

| data_type | 单元格值 | 使用方式 |
|---|---|---|
| string | 字符串 | 普通文本、备注 |
| int | 整数 | 排名、计数 |
| float | 有限数字 | 金额、比率 |
| bool | true / false | 是否条件 |
| date | ISO 日期或带时区的时间 | 日期、时间 |
| people | `{name, image_url?, url?, identity?}` | 人物、个人创作者；头像和名称在同一单元格 |
| company | `{name, image_url?, url?, identity?}` | 公司、品牌、组织；Logo 和名称在同一单元格 |
| website | http(s) URL 字符串 | 需要独立展示的网站链接 |
| email | 邮箱字符串 | 邮箱地址 |
| telephone | 电话字符串 | 联系电话 |
| score | `{level, reason?}` | 评级徽章，悬停查看评价理由 |

所有类型均可用 `null` 表示未知。people/company 的 `name` 必填且非空，两个 URL 字段可省略；URL 使用 http 或 https。`image_url` 应指向已获得的图片，不要填写普通主页或臆造头像地址。

### 富单元格优先

人物或品牌资料使用 people/company：名称和头像/Logo 在同一个对象里，不能拆成普通文字列和头像链接列。创作者搜索结果固定三列：创作者 people、频道链接 website、匹配评价 score；品牌专属名单可将 people 改为 company。一般表格按用户要求选择业务列。

同一列混合个人和品牌账号时保持统一的富类型展示，不降级为纯文字。

评分的 `level` 只能是 `excellent`、`good`、`medium`、`low`；解释写入 `reason`，不要默认另建“评价”或“评分理由”列。独立简介、用户明确要求的备注可以单列。创作者/频道搜索名单默认包含“匹配评价”score，评价本次需求的匹配程度；其他一般表格不自动增加评分。

不要因为名称含有“头像”“评分”等词就假设网页会自动合并：实际渲染取决于 data_type。people/company/score 当前不支持排序。

## 调用流程

1. 创建新表用 `table_create`，只定义完成任务所需的列。操作已有表先用 `table_list` 定位，再用 `table_get` 查看真实列 ID 和类型。
2. 新表创建结果也包含真实列 ID；已有结果足够时无需重复查询。行 `values` 的键必须是返回的列 ID，不能是列名。
3. 用 `table_create_rows` 写入记录；用 `table_update_rows` 按真实行 ID 局部更新。省略的字段保持不变，明确清空用 null。每次最多 100 行；加列每次最多 100 列。
4. 查看内容用 `table_query_rows`，根据 `has_more` 翻页；页码从 1 开始，size 最大 100。不要为了展示数据去浏览网页抓取。
5. 创建表、读取表、加列返回 `web_url`。创建/更新行返回 `{table_id, web_url, rows}`（MCP 位于 structuredContent.result）。向用户提供原样返回的可点击详情链接，并说明本次成功写入的记录数；只建表时说明是空表。

请求失败或结果不确定时，先查询确认当前状态，再决定是否重试，避免重复建表或重复写入。不要自行改造现有表结构来纠正建模，除非用户已要求。

## 最小例子

建表参数：

```json
{
  "name": "候选创作者",
  "columns": [
    {"name": "创作者", "data_type": "people"},
    {"name": "频道链接", "data_type": "website"},
    {"name": "匹配评价", "data_type": "score"}
  ]
}
```

以下仅展示值格式；实际调用必须把示意的列 ID 替换为建表或查表结果，内容来自用户提供的数据或已核实的来源：

```json
{
  "table_id": "<返回的表ID>",
  "rows": [{"values": {
    "<创作者列ID>": {"name": "示例创作者", "image_url": "https://example.com/avatar.png", "url": "https://example.com/profile"},
    "<频道链接列ID>": "https://example.com/profile",
    "<匹配评价列ID>": {"level": "good", "reason": "与本次筛选条件相关的评价依据"}
  }}]
}
```

这里的地址和数值都是格式示例，不要当作真实资料写入。没有头像来源就省略 image_url；资料不足的值用 null。插件本身不提供自动检索头像或自动补全资料的能力。

## 隐藏的平台身份

people/company 支持可选 `identity: {platform, platform_id}`。已知平台原生 ID 时必须随富字段原样保存，例如 YouTube 搜索返回的 platform_id；`platform` 写 `youtube`。名称、头像、URL 和 identity 在同一对象中，不另建平台 ID 列。界面不展示 identity，但读取记录会返回它。手动录入且没有可靠身份时省略 identity，禁止用名称、handle 或 URL 猜测原生 ID。编辑已有富字段时保留原 identity，除非明确更换实体。

当前查询结果表相互独立；保存身份仅用于追溯与后续达人库接入，不启用跨表或表内唯一性约束。
