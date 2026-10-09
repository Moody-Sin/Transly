import fs from 'node:fs'

const msgFile = process.argv[2]
if (!msgFile) {
  process.exit(0)
}

const content = fs.readFileSync(msgFile, 'utf8')
const firstLine = content.trim().split('\n')[0]?.trim() || ''

// 允许合并分支等默认提交说明
if (firstLine.startsWith('Merge branch ') || firstLine.startsWith('Merge remote-tracking branch ')) {
  process.exit(0)
}

const pattern = /^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-zA-Z0-9_\-\./]+\))?!?: .+/

if (!pattern.test(firstLine)) {
  console.error('\n\x1b[31m[commit-msg 拒绝] 提交说明不符合 Conventional Commits 约定式提交规范：\x1b[0m')
  console.error(`  ${firstLine}\n`)
  console.error('\x1b[33m格式应为: <type>(<scope>): <subject> 或 <type>: <subject>\x1b[0m')
  console.error('\x1b[33m示例: feat(ui): 主输入框增加快捷清空 / fix(engine): 修复网络重试退避\x1b[0m\n')
  process.exit(1)
}
