export function menuPosition(rect:{top:number;bottom:number;left:number;width:number},width:number,height:number,contentHeight:number){
 const below=Math.max(0,height-rect.bottom-14),above=Math.max(0,rect.top-14);
 const desired=Math.min(240,contentHeight),up=below<desired&&above>below;
 return {left:Math.max(8,Math.min(rect.left,width-rect.width-8)),width:Math.min(rect.width,Math.max(0,width-16)),maxHeight:Math.min(desired,up?above:below),...(up?{bottom:Math.max(8,height-rect.top+6)}:{top:Math.max(8,rect.bottom+6)})};
}
