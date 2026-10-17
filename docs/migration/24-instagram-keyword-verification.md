# Instagram 关键词搜索复验

2026-10-17，用户授权直接尝试 Instagram 搜索并跳过私密账号。沿用原 OpenCLI 身份 jga2zn4u。搜索适配器已过滤原生字段 is_private=true 的结果；未返回私密标记不等于已证明账号公开，后续仍须以主页明确状态为准。

## 实际结果

关键词 `makeup tutorial` 返回 5 条账号身份：make_up_tutorials_videos、__makeupstyles、talents_makeup、sylviataufique、makeupp__12。返回字段只有原生 author_id、username、name、url。未获取粉丝量、简介、原创正文、互动量或公开邮箱。

搜索回执：`.scratch/four-platform-instagram-search-e35e890c-d994-491f-8a47-f538b1e07b01.json`。

选择名称明确指向个人创作者的 sylviataufique 读取原生主页，返回 HTTP 429。随即停止该平台读取，未切换身份、入口或关键词继续尝试。回执：`.scratch/four-platform-instagram-native-a6a81f2a-e101-463c-b9bc-9678f45a81a8.json`。

## 关键词调整

当前入口是账号搜索，匹配用户名和显示名称，不能把关键词命中当成公开帖子或原创证明。`makeup tutorial` 容易命中教程合集，后续优先使用体现个人职业或身份的 `bridal makeup artist`、`beauty content creator`、`skincare educator`。这些关键词尚未实测，不标记为已验证优化；本轮限流后没有继续执行。

关键词无法保证排除私密账号。保留搜索阶段原生私密字段过滤；主页明确为私密时跳过，不请求关注、不读取其受限内容。私密状态未知和资料读取限流分别记录，不能互相替代。

Instagram 搜索身份返回验收通过，完整评分证据仍待核实。
