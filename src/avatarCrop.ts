export function avatarInitials(name:string):string {
 const words=name.trim().split(/\s+/).filter(Boolean);
 if(!words.length)return 'NC';
 return (words.length>1?Array.from(words[0])[0]+Array.from(words[words.length-1])[0]:Array.from(words[0]).slice(0,2).join('')).toLocaleUpperCase();
}
export function cropRect(width:number,height:number,zoom:number,x:number,y:number){
 const size=Math.min(width,height)/Math.max(1,Math.min(4,zoom));
 return {x:(width-size)*(1+Math.max(-1,Math.min(1,x)))/2,y:(height-size)*(1+Math.max(-1,Math.min(1,y)))/2,size};
}
