import { useEffect, useState } from 'react';

// 以完整桌面布局为基准，宽高使用同一比例，避免缩放时挤出回放按钮。
export function useWindowScale() {
  const measure = () => Math.min(window.innerWidth / 1200, window.innerHeight / 800);
  const [scale, setScale] = useState(measure);

  useEffect(() => {
    const resize = () => setScale(measure());
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);

  return scale;
}
