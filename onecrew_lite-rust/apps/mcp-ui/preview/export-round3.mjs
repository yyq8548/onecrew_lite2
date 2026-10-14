import {build} from 'esbuild';
import {writeFile} from 'node:fs/promises';
const result=await build({stdin:{contents:"export {round3Cases,round3Initial} from './src/test-support/round3';",resolveDir:process.cwd(),loader:'ts'},bundle:true,platform:'node',format:'esm',write:false});
const {round3Cases,round3Initial}=await import('data:text/javascript;base64,'+Buffer.from(result.outputFiles[0].text).toString('base64'));
await writeFile('dist/round3/20-cases.json',JSON.stringify({mode:'local-simulation-only',cases:round3Cases.map(c=>({...c,initialState:round3Initial(c.id)}))},null,2));
await writeFile('docs/ROUND-3.md','# 第三轮功能实验\n\n20 条分支，补充保存、评分、问答与项目的连续操作。所有回执仅为本地模拟，未接入 OneCrew 或消息服务。沿用共享组件及现有主题规则。\n\n'+round3Cases.map(c=>`## ${c.id}. ${c.title}\n\n${c.steps}\n`).join('\n')+'\n构建：node preview/build-round3.mjs && node preview/export-round3.mjs。打开 dist/round3/index.html。评分结果使用固定值，不代表真实模型评估；自动检查覆盖初始渲染与部分结果映射，实际交互视觉仍需手动验收。\n');
