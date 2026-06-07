// 设计审查循环·单轮 workflow（/goal 主循环与 cron 兜底共用）
// 调用：Workflow({ scriptPath: 'docs/review/round-workflow.mjs', args: { round: k } })
// 流程：8维+3盲点 独立审查 → 每条 independent verifier 复核 → confirmed 按文件分组 fix-agent 就地 Edit → 写轮报告
// 返回：compact 摘要（counts + slim findings），主会话据此更新 findings-ledger.md
export const meta = {
  name: 'design-review-round',
  description: '设计审查循环·单轮：8维+3盲点对抗审查→独立verifier复核→按文件分组fix-agent就地Edit→写轮报告',
  phases: [
    { title: '审查' },
    { title: '验证' },
    { title: '修复' },
    { title: '报告' },
  ],
}

const ROUND = (args && args.round) || 1
const RP = 'docs/review/REVIEW_LOOP.md'

const COMMON = [
  '项目：Rust 迁移验证工作台的设计文档仓库（纯设计，v0.9.4）。产出物 = Claude Code Plugin + rustmigrate CLI，帮助把 TS/Python/C 项目迁移到 Rust。',
  '审查目标与规则以 ' + RP + ' 为唯一权威：聚焦【迁移质量 + 工程 + 开源成熟度】，不碰企业级合规/治理。',
  '纪律（务必遵守）：',
  '- 这套设计已过 7+ 轮审查、非常成熟，已显式处理大量问题（HashMap 迭代顺序、整数溢出、UTF-8/16、异步取消安全、Send/Sync 传染、unsafe 分级、断点续传、上下文预算等）。只报真实缺口，别把"已处理项"当问题。',
  '- 本仓库有过 LLM 幻觉先例（虚构案例/工具/论文）。每条结论必须能在文档定位依据，禁止臆造。',
  '- **不得**提"仅需实现期实证数据(benchmark/实测基准/性能数据/批大小实测/定量验证/可行性实测)"类发现——这类转入 M0 spike 清单，不作为设计缺陷、不要求新增正文。只报真正的设计级缺陷：自相矛盾/定义缺失/选型不当/流程遗漏/跨文件不一致/逻辑漏洞。',
  '- **R3 起收口**：优先净删除（去重/合并/删冗余），原则上不整段新增。',
  '- **防尾部震荡**：前轮已修(fixed)的 high 不得在下轮作为"新 high"重新提出，除非修复本身引入了比原问题更严重的全新缺陷（而非"修复不完美/可以更精确"级别的微瑕）。"fix-induced micro-contradiction"（如数字 off-by-one、格式轻微不一致）最多报 medium，不升 high。',
  '- 严重度定义见 ' + RP + ' §2；护栏见 §5。',
].join('\n')

const FINDINGS_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['id', 'title', 'severity', 'location', 'problem', 'why', 'fix', 'confidence'],
        properties: {
          id: { type: 'string' },
          title: { type: 'string' },
          severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
          location: { type: 'string' },
          problem: { type: 'string' },
          why: { type: 'string' },
          fix: { type: 'string' },
          confidence: { type: 'number' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['id', 'verdict', 'reason', 'fix_soundness'],
  properties: {
    id: { type: 'string' },
    verdict: { type: 'string', enum: ['confirmed', 'adjusted', 'rejected'] },
    reason: { type: 'string' },
    adjusted_severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
    fix_soundness: { type: 'string', enum: ['sound', 'partial', 'unsound'] },
    improved_fix: { type: 'string' },
  },
}

const FIX_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['file', 'applied'],
  properties: {
    file: { type: 'string' },
    applied: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false, required: ['id', 'action', 'summary'],
        properties: {
          id: { type: 'string' },
          action: { type: 'string', enum: ['edited', 'accepted', 'skipped'] },
          summary: { type: 'string' },
        },
      },
    },
    notes: { type: 'string' },
  },
}

const DIMS = [
  { key: 'D1', title: '迁移质量与翻译方法论', files: 'docs/design/01-positioning-and-methodology.md, docs/design/03-execution-model.md, docs/design/07-pitfalls-and-risks.md' },
  { key: 'D2', title: '验证体系可靠性', files: 'docs/design/03-execution-model.md(§7), docs/design/04-toolchain.md, docs/design/07-pitfalls-and-risks.md' },
  { key: 'D3', title: '工具架构与工程质量', files: 'docs/design/02-architecture.md, docs/design/06-plugin-structure.md, docs/design/09-appendix-schemas.md' },
  { key: 'D4', title: '技术选型审查', files: 'docs/design/04-toolchain.md, docs/design/06-plugin-structure.md(§10.0.1)' },
  { key: 'D5', title: '编排可靠性与确定性', files: 'docs/design/06-plugin-structure.md(§10.5), docs/design/02-architecture.md(§3.4), docs/design/09-appendix-schemas.md' },
  { key: 'D6', title: '规模化与性能', files: 'docs/design/04-toolchain.md(§5.7), docs/design/02-architecture.md(§3.5), docs/design/03-execution-model.md' },
  { key: 'D7', title: '可维护性/可扩展性/社区贡献', files: 'docs/design/05-documentation-system.md, docs/design/06-plugin-structure.md(§11), docs/design/04-toolchain.md(§5.5)' },
  { key: 'D8', title: '范围控制/过度设计/路线图', files: 'docs/design/08-roadmap-and-reference.md, docs/design/01-positioning-and-methodology.md, docs/design/README.md' },
]
const BLINDS = [
  { key: 'BS1', title: '实操盲点（真实迁移开源项目会在哪崩）' },
  { key: 'BS2', title: 'OSS 工程基线（CI/release/dogfooding/eval/性能基准/错误信息/可复现）' },
  { key: 'BS3', title: '内部矛盾/跨文件一致性/逻辑漏洞' },
]

function reviewPrompt(item, isBlind) {
  const base = COMMON + '\n\n你是审查者，负责【' + item.key + ' ' + item.title + '】。\n第一步：用 Read 打开 ' + RP + '，读 §0/§2/§3/§5（目标、严重度、你这一维的 lens、护栏）。'
  if (isBlind) {
    return base + '\n第二步：通读 docs/design/ 下全部 .md，按 §3 中 ' + item.key + ' 的角度，找出 8 个固定维度可能漏掉的问题。\n输出 0-5 个高质量 finding（质量优先，宁缺毋滥，没有真问题就返回空）。id 形如 ' + item.key + '-01；每条必须给具体可落地的优化方案，location 用 文件:章节。'
  }
  return base + '\n第二步：用 Read/Grep 重点阅读：' + item.files + '（可交叉印证其他文件）。\n输出 0-6 个高质量 finding（质量优先，宁缺毋滥，没有真问题就返回空）。id 形如 ' + item.key + '-01；每条必须给具体可落地的优化方案，location 用 文件:章节。'
}

function verifyPrompt(f, dim) {
  return COMMON + '\n\n你是对抗式验证者，独立复核下面这条发现。必须用 Read/Grep 打开它 location 指向的位置核实：(1) 问题确实存在且未被文档充分处理（不是误读、不是设计已处理）；(2) 严重度是否准确（不准则给 adjusted_severity）；(3) 优化方案是否合理可行（可改进则给 improved_fix）。默认怀疑——证据不足或属误读判 rejected。\n\n维度：' + dim + '\nID: ' + f.id + '\n标题: ' + f.title + '\n严重度: ' + f.severity + '\n位置: ' + f.location + '\n问题: ' + f.problem + '\n为什么: ' + f.why + '\n方案: ' + f.fix + '\n\n输出 verdict。'
}

function fixPrompt(file, findings) {
  const list = findings.map(function (f) {
    const plan = (f.verdict && f.verdict.improved_fix) ? f.verdict.improved_fix : f.fix
    return '- [' + f.id + '](' + ((f.verdict && f.verdict.adjusted_severity) || f.severity) + ') ' + f.title + ' @ ' + f.location + '\n  问题: ' + f.problem + '\n  方案: ' + plan
  }).join('\n')
  return COMMON + '\n\n你负责修复 docs/design/' + file + ' 中的以下已确认发现。先 Read 相关章节，再用 **Edit 就地最小改动**（禁止整文件 Write，防卡死）落实优化方案。\n护栏（见 ' + RP + ' §5）：不扩范围、能简化就简化、守 CLAUDE.md「文件权威来源」表（改动同一信息只改权威文件并同步引用）。\nblocker/high 必须改；medium/low 可改；若选择不改某条 medium/low，必须在文档对应处就地追加一句「PORT-REVIEW 接受：<理由>」，不允许留空。\n\n待修复发现：\n' + list + '\n\n完成后按 schema 报告每条处理（edited/accepted/skipped + 一句摘要）。'
}

// ---- 审查 + 验证（pipeline：每维审查完即开始验证，不等其他维度）----
phase('审查')
const ALL = DIMS.map(function (d) { return Object.assign({}, d, { blind: false }) })
  .concat(BLINDS.map(function (b) { return Object.assign({}, b, { blind: true }) }))

const reviewed = await pipeline(
  ALL,
  function (item) {
    return agent(reviewPrompt(item, item.blind), { label: '审查:' + item.key, phase: '审查', schema: FINDINGS_SCHEMA, agentType: 'Explore' })
  },
  function (res, item) {
    const fs = (res && Array.isArray(res.findings)) ? res.findings : []
    if (!fs.length) return Promise.resolve({ key: item.key, title: item.title, findings: [] })
    return parallel(fs.map(function (f) {
      return function () {
        return agent(verifyPrompt(f, item.key + ' ' + item.title), { label: '验证:' + f.id, phase: '验证', schema: VERDICT_SCHEMA, agentType: 'Explore' })
          .then(function (v) { return Object.assign({}, f, { dim: item.title, key: item.key, verdict: v }) })
          .catch(function () { return null })
      }
    })).then(function (arr) { return { key: item.key, title: item.title, findings: arr.filter(Boolean) } })
  }
)

const all = reviewed.filter(Boolean).reduce(function (acc, r) { return r && r.findings ? acc.concat(r.findings) : acc }, [])
function effSev(f) { return (f.verdict && f.verdict.adjusted_severity) ? f.verdict.adjusted_severity : f.severity }
const confirmed = all.filter(function (f) { return f.verdict && (f.verdict.verdict === 'confirmed' || f.verdict.verdict === 'adjusted') })

function fileOf(f) {
  const loc = (f.location || '') + ''
  const m = loc.match(/(\d\d-[a-z0-9\-]+\.md|README\.md|index\.html)/i)
  return m ? m[1] : null
}
const byFile = {}
confirmed.forEach(function (f) { const k = fileOf(f); if (k) { (byFile[k] = byFile[k] || []).push(f) } })
const ungrouped = confirmed.filter(function (f) { return !fileOf(f) })

// ---- 修复：每个文件一个 fix-agent（文件互不相交，可并行）----
phase('修复')
const fileKeys = Object.keys(byFile)
const fixes = await parallel(fileKeys.map(function (fk) {
  return function () {
    return agent(fixPrompt(fk, byFile[fk]), { label: '修复:' + fk, phase: '修复', schema: FIX_SCHEMA }).catch(function () { return null })
  }
}))

// ---- 报告：写详细轮报告到文件（不进主会话上下文）----
phase('报告')
function slim(f) {
  return { id: f.id, dim: f.dim || f.key, severity: effSev(f), file: fileOf(f) || '(跨文件/未定位)', title: f.title, verdict: f.verdict ? f.verdict.verdict : '(无)' }
}
const reportPayload = JSON.stringify({
  round: ROUND,
  findings: all.map(function (f) {
    return Object.assign({}, slim(f), {
      problem: f.problem, why: f.why,
      plan: (f.verdict && f.verdict.improved_fix) || f.fix,
      verdict_reason: f.verdict ? f.verdict.reason : '',
    })
  }),
  fixes: fixes.filter(Boolean),
})
await agent(
  '把本轮审查写成报告文件，用 Write 写到 docs/review/latest-round-report.md，标题「# 本轮审查报告」。按维度分组列出每条 finding：ID / 严重度 / 位置 / 状态(confirmed|adjusted|rejected) / 问题(1-2句) / 优化方案(1-2句)；被 rejected 的也要列并附 verifier 理由（为什么不成立）。末尾附「本轮修复文件清单」与「转 M0 spike 清单」（被排除的实证类诉求，若有）。务必简洁——每条 finding 控制在 3-4 行，避免文件过大导致写入卡死。数据(JSON)：\n' + reportPayload,
  { label: '写轮报告', phase: '报告' }
)

function countConfirmed(s) { return confirmed.filter(function (f) { return effSev(f) === s }).length }
return {
  round: null,
  counts: {
    raised: all.length,
    confirmed: confirmed.length,
    rejected: all.filter(function (f) { return f.verdict && f.verdict.verdict === 'rejected' }).length,
    blocker: countConfirmed('blocker'),
    high: countConfirmed('high'),
    medium: countConfirmed('medium'),
    low: countConfirmed('low'),
  },
  confirmed: confirmed.map(slim),
  rejected: all.filter(function (f) { return f.verdict && f.verdict.verdict === 'rejected' }).map(function (f) { return { id: f.id, title: f.title, reason: f.verdict.reason } }),
  ungrouped: ungrouped.map(slim),
  filesFixed: fileKeys,
  reportPath: 'docs/review/latest-round-report.md',
}
