import test from 'node:test'
import assert from 'node:assert/strict'
import { detectDocumentFormat, generateExportFilename, formatFileSize } from './document.ts'

test('detectDocumentFormat: 根据扩展名识别合法格式', () => {
  assert.equal(detectDocumentFormat('test.txt'), 'txt')
  assert.equal(detectDocumentFormat('NOTES.TXT'), 'txt')
  assert.equal(detectDocumentFormat('doc.md'), 'md')
  assert.equal(detectDocumentFormat('SPEC.MARKDOWN'), 'md')
  assert.equal(detectDocumentFormat('subtitle.srt'), 'srt')
  assert.equal(detectDocumentFormat('movie.SRT'), 'srt')
  assert.equal(detectDocumentFormat('messages.json'), 'json')
  assert.equal(detectDocumentFormat('INDEX.HTML'), 'html')
  assert.equal(detectDocumentFormat('about.htm'), 'html')

  // 非法或未支持格式
  assert.equal(detectDocumentFormat('image.png'), null)
  assert.equal(detectDocumentFormat('script.js'), null)
  assert.equal(detectDocumentFormat('archive.tar.gz'), null)
  assert.equal(detectDocumentFormat(''), null)
})

test('generateExportFilename: 正确注入目标语言标识', () => {
  assert.equal(generateExportFilename('readme.md', 'zh-CN'), 'readme.zh-CN.md')
  assert.equal(generateExportFilename('video.srt', 'en'), 'video.en.srt')
  assert.equal(generateExportFilename('locales.json', 'zh-CN'), 'locales.zh-CN.json')
  assert.equal(generateExportFilename('page.html', 'en'), 'page.en.html')
  assert.equal(generateExportFilename('path/to/doc.txt', 'ja'), 'path/to/doc.ja.txt')
  assert.equal(generateExportFilename('LICENSE', 'fr'), 'LICENSE.fr.txt')
})

test('formatFileSize: 格式化不同体量文件大小', () => {
  assert.equal(formatFileSize(500), '500 B')
  assert.equal(formatFileSize(2048), '2.0 KB')
  assert.equal(formatFileSize(1048576 * 2.5), '2.5 MB')
})
