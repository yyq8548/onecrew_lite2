import {useId} from 'react';
import Select from 'react-select';
import countryList from 'react-select-country-list';

const names=new Intl.DisplayNames(['zh-CN'],{type:'region'});
export const countryOptions=countryList().getData().map(country=>({value:country.value,label:names.of(country.value)??country.label,english:country.label})).sort((a,b)=>a.label.localeCompare(b.label,'zh-CN'));
export function CountryPicker({value,onChange}:{value:string;onChange:(value:string)=>void}){
 const id=useId();const labels=value.split(/[,，、]/).map(v=>v.trim()).filter(Boolean);
 const selected=labels.map(label=>countryOptions.find(option=>option.label===label||option.value===label||option.english===label)??{value:label,label,english:label});
 return <div className="country-picker"><label htmlFor={id}>地区</label><Select inputId={id} instanceId={id} classNamePrefix="country" unstyled isMulti isSearchable isClearable options={countryOptions} value={selected} onChange={options=>onChange(options.map(option=>option.label).join(','))} placeholder="选择国家或地区" noOptionsMessage={()=>'没有匹配的国家或地区'} filterOption={(option,query)=>`${option.label} ${option.data.english} ${option.value}`.toLowerCase().includes(query.trim().toLowerCase())} menuPlacement="auto" menuPosition="fixed" menuPortalTarget={typeof document==='undefined'?undefined:document.body} styles={{menuPortal:base=>({...base,zIndex:100})}} maxMenuHeight={240} closeMenuOnSelect={false}/></div>;
}
