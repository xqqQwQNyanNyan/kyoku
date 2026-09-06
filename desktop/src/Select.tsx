import { useEffect, useId, useRef, useState } from 'react';

type Option = { value: string; label: string; description?: string };

export function Select({
  id,
  label,
  value,
  options,
  disabled = false,
  autoFocus = false,
  onChange,
}: {
  id?: string;
  label: string;
  value: string;
  options: Option[];
  disabled?: boolean;
  autoFocus?: boolean;
  onChange: (value: string) => void;
}) {
  const listId = useId();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [placement, setPlacement] = useState({ above: false, height: 300 });
  const selected = options.findIndex((option) => option.value === value);
  const expanded = open && !disabled;

  function show() {
    const button = trigger.current;
    if (!button || disabled || !options.length) return;
    const rect = button.getBoundingClientRect();
    // 菜单在缩放后的应用内定位，先把窗口剩余空间换回布局尺寸。
    const scale = rect.height / button.offsetHeight || 1;
    const below = window.innerHeight - rect.bottom;
    const above = below < Math.min(300, options.length * 52 + 12) * scale && rect.top > below;
    setPlacement({
      above,
      height: Math.min(300, Math.max(80, (above ? rect.top : below) / scale - 16)),
    });
    setActive(Math.max(0, selected));
    setOpen(true);
  }

  function choose(index: number) {
    if (options[index]) onChange(options[index].value);
    setOpen(false);
  }

  useEffect(() => {
    if (!expanded) return;
    function outside(event: PointerEvent) {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    }
    const close = () => setOpen(false);
    document.addEventListener('pointerdown', outside);
    window.addEventListener('resize', close);
    return () => {
      document.removeEventListener('pointerdown', outside);
      window.removeEventListener('resize', close);
    };
  }, [expanded]);

  useEffect(() => {
    const menu = list.current;
    const option = menu?.children[active] as HTMLElement | undefined;
    if (!expanded || !menu || !option) return;
    // 只滚动选项列表，避免带动牌桌或弹窗。
    if (option.offsetTop < menu.scrollTop) menu.scrollTop = option.offsetTop;
    else if (option.offsetTop + option.offsetHeight > menu.scrollTop + menu.clientHeight)
      menu.scrollTop = option.offsetTop + option.offsetHeight - menu.clientHeight;
  }, [active, expanded]);

  return (
    <div className="custom-select" ref={root}>
      <button
        ref={trigger}
        id={id}
        type="button"
        className="select-trigger"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={expanded}
        aria-controls={expanded ? listId : undefined}
        aria-activedescendant={expanded ? `${listId}-${active}` : undefined}
        disabled={disabled || !options.length}
        autoFocus={autoFocus}
        onClick={() => (expanded ? setOpen(false) : show())}
        onBlur={() => setOpen(false)}
        onKeyDown={(event) => {
          switch (event.key) {
            case 'ArrowDown':
            case 'ArrowUp':
              event.preventDefault();
              event.stopPropagation();
              if (!expanded) show();
              else
                setActive((current) =>
                  Math.max(
                    0,
                    Math.min(options.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)),
                  ),
                );
              break;
            case 'Home':
            case 'End':
              event.preventDefault();
              event.stopPropagation();
              if (!expanded) show();
              setActive(event.key === 'Home' ? 0 : options.length - 1);
              break;
            case 'Enter':
            case ' ':
              event.preventDefault();
              event.stopPropagation();
              if (expanded) choose(active);
              else show();
              break;
            case 'Escape':
              if (expanded) {
                event.preventDefault();
                event.stopPropagation();
                setOpen(false);
              }
              break;
            case 'Tab':
              setOpen(false);
          }
        }}
      >
        <span>{options[selected]?.label ?? '请选择'}</span>
        <svg aria-hidden="true" viewBox="0 0 12 12" className="select-chevron">
          <path d="m3 4.5 3 3 3-3" />
        </svg>
      </button>
      {expanded && (
        <ul
          ref={list}
          id={listId}
          role="listbox"
          aria-label={label}
          className={`select-menu ${placement.above ? 'above' : ''}`}
          style={{ maxHeight: placement.height }}
        >
          {options.map((option, index) => (
            <li
              key={option.value}
              id={`${listId}-${index}`}
              role="option"
              aria-selected={option.value === value}
              aria-label={option.label}
              className={index === active ? 'active' : ''}
              onPointerMove={() => setActive(index)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              <div>
                <span>{option.label}</span>
                {option.description && <small>{option.description}</small>}
              </div>
              <span className="select-check" aria-hidden="true">
                {option.value === value ? '✓' : ''}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
