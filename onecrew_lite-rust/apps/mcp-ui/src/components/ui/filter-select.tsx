import './filter-select.css';
import {useEffect,useLayoutEffect,useId,useRef,useState} from 'react';
import {createPortal} from 'react-dom';
import {menuPosition} from './menu-position';
import {ChevronDown,Check} from 'lucide-react';

export function FilterSelect({label,value,options,onChange,disabled=false}:{disabled?:boolean;label:string;value:string;options:{value:string;label:string}[];onChange:(value:string)=>void}){
 const id=useId();const root=useRef<HTMLDivElement>(null);const [open,setOpen]=useState(false);const [active,setActive]=useState(0);
 const menu=useRef<HTMLDivElement>(null);const trigger=useRef<HTMLButtonElement>(null);
 const [position,setPosition]=useState<ReturnType<typeof menuPosition>>();
 useLayoutEffect(()=>{if(!open)return;const update=()=>{if(trigger.current)setPosition(menuPosition(trigger.current.getBoundingClientRect(),window.innerWidth,window.innerHeight,menu.current?.scrollHeight??240));};update();window.addEventListener('resize',update);document.addEventListener('scroll',update,true);return()=>{window.removeEventListener('resize',update);document.removeEventListener('scroll',update,true);};},[open]);
 useEffect(()=>{if(disabled)setOpen(false);},[disabled]);
 const selected=Math.max(0,options.findIndex(option=>option.value===value));
 useEffect(()=>{if(!open)return;const close=(event:PointerEvent)=>{if(!root.current?.contains(event.target as Node)&&!menu.current?.contains(event.target as Node))setOpen(false);};document.addEventListener('pointerdown',close);return()=>document.removeEventListener('pointerdown',close);},[open]);
 useEffect(()=>{const list=menu.current,item=document.getElementById(`${id}-${active}`);if(open&&list&&item){if(item.offsetTop<list.scrollTop)list.scrollTop=item.offsetTop;else if(item.offsetTop+item.offsetHeight>list.scrollTop+list.clientHeight)list.scrollTop=item.offsetTop+item.offsetHeight-list.clientHeight;}},[active,open,id,position]);
 const choose=(index:number)=>{if(disabled)return;if(options[index])onChange(options[index].value);setOpen(false);};
 return <div className="filter-select" ref={root}><span id={`${id}-label`} className="filter-select-label">{label}</span><button ref={trigger} disabled={disabled} type="button" role="combobox" aria-labelledby={`${id}-label ${id}-value`} aria-haspopup="listbox" aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={open?`${id}-${active}`:undefined} className="filter-select-trigger" onClick={()=>{setActive(selected);setOpen(!open);}} onKeyDown={event=>{
 if(event.key==='Tab'){setOpen(false);return;}
 if(event.key==='Escape'){if(open){event.preventDefault();event.stopPropagation();setOpen(false);}return;}
 if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){event.preventDefault();setOpen(true);setActive(event.key==='Home'?0:event.key==='End'?options.length-1:!open?selected:Math.max(0,Math.min(options.length-1,active+(event.key==='ArrowDown'?1:-1))));return;}
 if(event.key==='Enter'||event.key===' '){event.preventDefault();if(open)choose(active);else{setActive(selected);setOpen(true);}return;}
 if(event.key.length===1&&!event.ctrlKey&&!event.metaKey){const index=options.findIndex(option=>option.label.toLocaleLowerCase().startsWith(event.key.toLocaleLowerCase()));if(index>=0){event.preventDefault();setActive(index);setOpen(true);}}
 }}><span id={`${id}-value`}>{options[selected]?.label}</span><ChevronDown size={12} aria-hidden="true"/></button>{open&&createPortal(<div ref={menu} style={position??{visibility:"hidden"}} id={`${id}-list`} role="listbox" aria-labelledby={`${id}-label`} className="filter-select-menu">{options.map((option,index)=><div key={option.value} id={`${id}-${index}`} role="option" aria-selected={value===option.value} data-active={active===index} className="filter-select-option" onPointerMove={()=>setActive(index)} onMouseDown={event=>event.preventDefault()} onClick={()=>choose(index)}><span>{option.label}</span>{value===option.value&&<Check size={14} aria-hidden="true"/>}</div>)}</div>,document.body)}</div>;
}
