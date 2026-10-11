import {describe,it,expect} from 'vitest';
import {avatarInitials,cropRect} from './avatarCrop';
describe('avatar crop and initials',()=>{
 it('uses readable initials for names and handles',()=>{
  expect(avatarInitials('jamesupereal')).toBe('JA');
  expect(avatarInitials('James Smith')).toBe('JS');
  expect(avatarInitials('')).toBe('NC');
 });
 it('keeps a zoomed portrait or landscape crop inside the image',()=>{
  expect(cropRect(800,400,1,0,0)).toEqual({x:200,y:0,size:400});
  expect(cropRect(400,800,2,1,-1)).toEqual({x:200,y:0,size:200});
  expect(cropRect(400,400,4,-1,1)).toEqual({x:0,y:300,size:100});
 });
});
