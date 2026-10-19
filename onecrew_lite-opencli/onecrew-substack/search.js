import {cli, Strategy} from '@jackwener/opencli/registry';
import {ArgumentError, CommandExecutionError, TimeoutError} from '@jackwener/opencli/errors';
import {searchPosts} from './search.mjs';
cli({site:'onecrew-substack',name:'search',access:'read',browser:false,
  domain:'substack.com',strategy:Strategy.PUBLIC,description:'Bounded Substack posts with canonical publication sources',
  args:[{name:'query',positional:true,required:true},{name:'limit',type:'int',default:5}],
  columns:['title','author','url','post_id','publication_id','slug','provider'],
  func:async args => {try {return await searchPosts(args.query,args.limit);} catch(error) {
    if (error.message === 'SUBSTACK_INPUT') throw new ArgumentError('Invalid Substack search input');
    if (error.name === 'TimeoutError' || error.name === 'AbortError') throw new TimeoutError('Substack public search',15);
    if (error.message === 'SUBSTACK_RISK') throw new CommandExecutionError('Substack rate limit requires waiting');
    throw new CommandExecutionError('Substack search response could not be verified');
  }} });
