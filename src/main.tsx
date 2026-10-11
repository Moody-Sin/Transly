import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import FloatWindow from './FloatWindow'
import './index.css'

const isFloatWindow = window.location.hash === '#float'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>{isFloatWindow ? <FloatWindow /> : <App />}</React.StrictMode>,
)
