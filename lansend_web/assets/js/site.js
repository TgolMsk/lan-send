// 官网的全部脚本：移动端菜单、标签页、复制按钮、首屏的局域网星座背景。
// 无依赖，无外部请求；关掉 JavaScript 时页面仍然完整可读（标签页会全部展开）。

(() => {
  'use strict'

  const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches

  /* ---------- 移动端菜单 ---------- */

  const toggle = document.querySelector('.nav-toggle')
  const nav = document.getElementById('site-nav')
  if (toggle && nav) {
    const isMobile = () => matchMedia('(max-width: 900px)').matches
    const setOpen = (open) => {
      toggle.setAttribute('aria-expanded', String(open))
      nav.hidden = !open
    }
    const sync = () => setOpen(!isMobile())
    sync()
    matchMedia('(max-width: 900px)').addEventListener('change', sync)
    toggle.addEventListener('click', () => setOpen(nav.hidden))
    nav.addEventListener('click', (e) => {
      if (e.target.closest('a') && isMobile()) setOpen(false)
    })
  }

  /* ---------- 标签页 ---------- */

  document.querySelectorAll('[data-tabs]').forEach((group) => {
    const buttons = [...group.querySelectorAll('[role="tab"]')]
    const panels = buttons.map((b) => document.getElementById(b.getAttribute('aria-controls')))
    const select = (index) => {
      buttons.forEach((b, i) => {
        b.setAttribute('aria-selected', String(i === index))
        b.tabIndex = i === index ? 0 : -1
        if (panels[i]) panels[i].hidden = i !== index
      })
    }
    buttons.forEach((button, i) => {
      button.addEventListener('click', () => select(i))
      button.addEventListener('keydown', (e) => {
        const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0
        if (!step) return
        e.preventDefault()
        const next = (i + step + buttons.length) % buttons.length
        select(next)
        buttons[next].focus()
      })
    })
    select(Math.max(0, buttons.findIndex((b) => b.getAttribute('aria-selected') === 'true')))
  })

  /* ---------- 复制命令 ---------- */

  document.querySelectorAll('.copy').forEach((button) => {
    button.addEventListener('click', async () => {
      const text = button.dataset.copy ?? ''
      try {
        await navigator.clipboard.writeText(text)
      } catch {
        // 没有剪贴板权限（http 或旧浏览器）时退回选中文本。
        const area = document.createElement('textarea')
        area.value = text
        document.body.append(area)
        area.select()
        try { document.execCommand('copy') } catch { /* 复制不了就算了 */ }
        area.remove()
      }
      const done = button.dataset.done
      const idle = button.dataset.idle ?? button.textContent
      button.dataset.idle = idle
      button.textContent = done ?? idle
      button.dataset.done = '1'
      setTimeout(() => {
        button.textContent = idle
        delete button.dataset.done
      }, 1600)
    })
  })

  /* ---------- 顶栏当前分区高亮 ---------- */

  const navLinks = [...document.querySelectorAll('#site-nav a[href^="#"]')]
  const sections = navLinks.map((a) => document.querySelector(a.getAttribute('href'))).filter(Boolean)
  if (sections.length && 'IntersectionObserver' in window) {
    const seen = new Set()
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => (entry.isIntersecting ? seen.add(entry.target) : seen.delete(entry.target)))
        const current = sections.find((s) => seen.has(s))
        navLinks.forEach((a) => {
          const on = current && a.getAttribute('href') === `#${current.id}`
          a.style.color = on ? 'var(--accent)' : ''
        })
      },
      { rootMargin: '-45% 0px -50% 0px' },
    )
    sections.forEach((s) => observer.observe(s))
  }

  /* ---------- 首屏背景：局域网里互相发现的设备 ---------- */

  const canvas = document.querySelector('.hero-net')
  if (canvas && canvas.getContext) {
    const ctx = canvas.getContext('2d')
    const nodes = []
    const packets = []
    let width = 0
    let height = 0
    let raf = 0

    const ink = () => (matchMedia('(prefers-color-scheme: dark)').matches ? [166, 166, 246] : [63, 63, 176])
    const mint = () => (matchMedia('(prefers-color-scheme: dark)').matches ? '27, 238, 121' : '18, 196, 99')

    const resize = () => {
      const rect = canvas.getBoundingClientRect()
      const dpr = Math.min(devicePixelRatio || 1, 2)
      width = rect.width
      height = rect.height
      canvas.width = Math.round(width * dpr)
      canvas.height = Math.round(height * dpr)
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const wanted = Math.min(64, Math.round((width * height) / 16000))
      while (nodes.length > wanted) nodes.pop()
      while (nodes.length < wanted) {
        nodes.push({
          x: Math.random() * width,
          y: Math.random() * height,
          vx: (Math.random() - 0.5) * 0.16,
          vy: (Math.random() - 0.5) * 0.16,
          r: 1.2 + Math.random() * 1.8,
        })
      }
    }

    const draw = () => {
      const [r, g, b] = ink()
      ctx.clearRect(0, 0, width, height)
      const reach = Math.min(190, Math.max(120, width / 8))

      for (let i = 0; i < nodes.length; i += 1) {
        const a = nodes[i]
        for (let j = i + 1; j < nodes.length; j += 1) {
          const c = nodes[j]
          const d = Math.hypot(a.x - c.x, a.y - c.y)
          if (d > reach) continue
          ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${(1 - d / reach) * 0.16})`
          ctx.lineWidth = 1
          ctx.beginPath()
          ctx.moveTo(a.x, a.y)
          ctx.lineTo(c.x, c.y)
          ctx.stroke()
        }
      }
      nodes.forEach((n) => {
        ctx.fillStyle = `rgba(${r}, ${g}, ${b}, 0.34)`
        ctx.beginPath()
        ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2)
        ctx.fill()
      })
      packets.forEach((p) => {
        const from = nodes[p.from]
        const to = nodes[p.to]
        if (!from || !to) return
        const x = from.x + (to.x - from.x) * p.t
        const y = from.y + (to.y - from.y) * p.t
        const fade = Math.sin(p.t * Math.PI)
        ctx.fillStyle = `rgba(${mint()}, ${0.85 * fade})`
        ctx.beginPath()
        ctx.arc(x, y, 2.6, 0, Math.PI * 2)
        ctx.fill()
      })
    }

    const step = () => {
      nodes.forEach((n) => {
        n.x += n.vx
        n.y += n.vy
        if (n.x < -20) n.x = width + 20
        if (n.x > width + 20) n.x = -20
        if (n.y < -20) n.y = height + 20
        if (n.y > height + 20) n.y = -20
      })
      for (let i = packets.length - 1; i >= 0; i -= 1) {
        packets[i].t += packets[i].speed
        if (packets[i].t >= 1) packets.splice(i, 1)
      }
      // 偶尔让一个“文件”沿着一条连线飞过去。
      if (packets.length < 3 && Math.random() < 0.02 && nodes.length > 4) {
        const from = Math.floor(Math.random() * nodes.length)
        let to = Math.floor(Math.random() * nodes.length)
        if (to === from) to = (to + 1) % nodes.length
        packets.push({ from, to, t: 0, speed: 0.004 + Math.random() * 0.004 })
      }
      draw()
      raf = requestAnimationFrame(step)
    }

    const start = () => {
      cancelAnimationFrame(raf)
      if (reduceMotion) draw()
      else raf = requestAnimationFrame(step)
    }

    resize()
    start()
    addEventListener('resize', () => {
      resize()
      if (reduceMotion) draw()
    })
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) cancelAnimationFrame(raf)
      else start()
    })
  }
})()
