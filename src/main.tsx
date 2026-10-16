import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import FloatWindow from './FloatWindow'
import ScreenshotOverlay from './ScreenshotOverlay'
import './index.css'

const hash = window.location.hash
const isFloatWindow = hash === '#float'
const isShotWindow = hash === '#shot'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    {isShotWindow ? <ScreenshotOverlay /> : isFloatWindow ? <FloatWindow /> : <App />}
  </React.StrictMode>,
)
