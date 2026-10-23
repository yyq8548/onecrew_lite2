import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {observeLinkedinPeople} from './linkedin-search.mjs';

function fixture() {
  const profile = 'https://www.linkedin.com/in/ashleymarsh-conexo/';
  const name = {href: profile, innerText: 'Ashley Marsh ', getClientRects: () => [1],
    querySelector: () => null, closest: selector => selector === 'p' ? {innerText: 'Ashley Marsh \n • 3rd+'} : null};
  const wrapper = {...name, innerText: 'Ashley Marsh is hiringAshley Marsh • 3rd+Founder @ Conexo',
    querySelector: () => name};
  const second = {...name, href: 'https://www.linkedin.com/in/ashleydhaines-/', innerText: 'Ashley Haines',
    closest: selector => selector === 'p' ? {innerText: 'Ashley Haines • 3rd+'} : null};
  const anchors = [wrapper, name, {...name, href: profile + 'opportunities/hiring-opportunities/view/'}, second];
  const context = {URL, location: {href: 'https://www.linkedin.com/search/results/people/?keywords=Ashley%20Marsh%20Conexo'},
    getComputedStyle: () => ({visibility: 'visible'}),
    document: {querySelector: () => ({querySelectorAll: () => anchors})}};
  const read = () => JSON.parse(JSON.stringify(vm.runInNewContext(
    `(${observeLinkedinPeople.toString()})('Ashley Marsh Conexo', 5)`, context)));
  return {context, anchors, name, read};
}

test('hiring wrapper cannot consume the nested visible author identity before deduplication', () => {
  const {read} = fixture();
  assert.deepEqual(read().map(row => [row.name, row.profile_url]), [
    ['Ashley Marsh', 'https://www.linkedin.com/in/ashleymarsh-conexo/'],
    ['Ashley Haines', 'https://www.linkedin.com/in/ashleydhaines-/'],
  ]);
});

test('ignores hidden, foreign, unrelated and duplicate profile links', () => {
  const {anchors, name, read} = fixture();
  anchors.push({...name}, {...name, href: 'https://evil.example/in/author/'},
    {...name, href: 'https://www.linkedin.com/in/hidden/', getClientRects: () => []},
    {...name, href: 'https://www.linkedin.com/in/unrelated/', closest: () => null});
  assert.equal(read().length, 2);
});

test('wrong query, redirected page and unrecognized empty markup fail rather than certifying zero results', () => {
  const f = fixture();
  for (const href of ['https://www.linkedin.com/login',
    'https://www.linkedin.com/search/results/people/?keywords=someone-else']) {
    f.context.location.href = href;
    assert.throws(f.read, /INVALID/);
  }
  const empty = fixture(); empty.anchors.length = 0;
  assert.throws(empty.read, /INVALID/);
});

test('headline belongs to this visible result, never a hidden or neighbouring person', () => {
  const {name, read} = fixture();
  const subtitle = {innerText:'Hardware reviewer and sensor tester',getClientRects:()=>[1]};
  const links=[name];
  const card={querySelectorAll:()=>links,querySelector:()=>subtitle};
  name.closest=selector=>selector==='p'?{innerText:'Ashley Marsh • 3rd+'}:card;
  assert.equal(read()[0].headline,subtitle.innerText);
  subtitle.getClientRects=()=>[];
  assert.equal(read()[0].headline,'');
  subtitle.getClientRects=()=>[1];
  links.push({href:'https://www.linkedin.com/in/someone-else/'});
  assert.equal(read()[0].headline,'');
});
