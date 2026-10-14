import {build} from 'esbuild';
import {writeFile} from 'node:fs/promises';
const result=await build({stdin:{contents:"export {round4Cases,round4Initial} from './src/test-support/round4';",resolveDir:process.cwd(),loader:'ts'},bundle:true,platform:'node',format:'esm',write:false});
const {round4Cases,round4Initial}=await import('data:text/javascript;base64,'+Buffer.from(result.outputFiles[0].text).toString('base64'));
await writeFile('dist/round4/20-cases.json',JSON.stringify({mode:'local-simulation-only',cases:round4Cases.map(c=>({...c,...(c.id<=15?{initialState:round4Initial(c.id)}:{inputFixture:c.id===18?Array.from({length:50},(_,i)=>`keyword ${i+1}`):c.id===19?'中'.repeat(1000):c.id===20?'  \n  ':['cooking']})}))},null,2));
await writeFile('docs/ROUND-4.md','# 第四轮：20 个新增边界问题\n\n对照前三轮（20 条状态、15 条流程、20 条功能分支）新增以下检查点。这是新增测试场景，不代表全部用户状态的覆盖率。复用正式展示与输入组件，业务结果为本地模拟。\n\n'+round4Cases.map(c=>`## ${c.id}. ${c.title}\n\n新增覆盖：${c.added}。\n\n操作与预期：${c.steps}\n`).join('\n')+'\n## 运行与验证\n\n构建：node preview/build-round4.mjs && node preview/export-round4.mjs。双击配套 HTML。数据变化按钮是测试控制，正式界面不显示。自动检查覆盖 20 个初始渲染、3 个审核变更、全员已入库与 HTML 文本转义；实际布局、键盘与图片失败回退需在页面手动验收。所有写入及评分仅为模拟，不证明真实后端已接入。\n');
