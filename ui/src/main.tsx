import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
// Single design-token source (DESIGN §2) — MUST load first so every consumer
// (shadcn/Radix panels AND the React Flow canvas) reads one palette.
import '../design-system/tokens.css'
import './index.css'
import App from './App.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
