# Facebook 空联系入口实测材料

2026-10-21从本机已登录浏览器读取公开主页`https://www.facebook.com/ericlofholm`，使用托管OpenCLI 1.8.8及当前账号的onecrew-facebook-profile native-read命令，原生账号ID为698714578。命令退出码为0，profile.json保存实际返回的公开资料，未加入模拟字段或账号凭证。

真实触发链：读取器返回空contact_links；facebookProfilePlan解码删除该字段；保存前validateDiscoveryRetainedProfile要求2.0.0版本包含该字段，抛出review_native_profile_binding_unavailable，主页回执停在reading。回归检查覆盖解码到保存前验证之间的完整字段传递。
