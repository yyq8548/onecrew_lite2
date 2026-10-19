import {describe,it,expect} from 'vitest';
import {menuPosition} from './menu-position';
describe('dropdown viewport placement',()=>{
 it('caps a long menu below the trigger',()=>{expect(menuPosition({top:100,bottom:140,left:20,width:300},800,800,1000)).toEqual({left:20,width:300,maxHeight:240,top:146});});
 it('flips above when the lower edge has insufficient space',()=>{expect(menuPosition({top:600,bottom:640,left:20,width:300},800,700,1000)).toEqual({left:20,width:300,maxHeight:240,bottom:106});});
 it('limits height to available space on small screens',()=>{expect(menuPosition({top:100,bottom:140,left:0,width:500},320,240,1000)).toEqual({left:8,width:304,maxHeight:86,top:146});});
 it('keeps short menus below when they fit',()=>{expect(menuPosition({top:500,bottom:540,left:700,width:200},800,700,80)).toEqual({left:592,width:200,maxHeight:80,top:546});});
});
