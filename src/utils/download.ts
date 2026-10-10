/**
 * 触发浏览器下载。
 *
 * 两个必须遵守的约束（Chromium / WebView2 实测）：
 * 1. `<a>` 必须先入 DOM，脱离文档树的点击不会触发导航下载；
 * 2. `revokeObjectURL` 必须让出一帧 —— 同步 revoke 会连带取消刚触发的下载。
 */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.style.display = 'none'
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
  setTimeout(() => URL.revokeObjectURL(url), 0)
}
