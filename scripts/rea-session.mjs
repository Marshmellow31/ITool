import { Client } from '../.tools/rea/node_modules/@modelcontextprotocol/client/dist/index.mjs';
import { StdioClientTransport } from '../.tools/rea/node_modules/@modelcontextprotocol/client/dist/stdio.mjs';
import { writeFile, readFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import readline from 'node:readline';

// Explicit, local process. Does not edit the user's global MCP registrations.
const root=path.resolve(import.meta.dirname,'..');
const transport=new StdioClientTransport({command:process.execPath,args:[path.join(root,'.tools/rea/node_modules/rea-agents/scripts/rea.mjs'),'mcp'],env:{...process.env,GHIDRA_INSTALL_DIR:path.join(root,'.tools/ghidra_12.1.4_PUBLIC'),JAVA_HOME:'C:\\Program Files\\Android\\Android Studio\\jbr',REA_ANALYSIS_PROVIDER:'ghidra'},stderr:'pipe'});
const client=new Client({name:'itool-local-analysis',version:'1.0.0'});
await mkdir(path.join(root,'analysis/rea'),{recursive:true});
await client.connect(transport);
const catalog=await client.listTools();
await writeFile(path.join(root,'analysis/rea/tool-catalog.json'),JSON.stringify(catalog,null,2));
console.log('REA MCP connected; tool schemas saved locally.');
if(process.argv[2]) {
    const calls=JSON.parse(await readFile(process.argv[2],'utf8'));
    for(const [i,call] of calls.entries()) {
        console.log(`Calling ${call.name}`);
        const result=await client.callTool({name:call.name,arguments:call.arguments},{timeout:360000});
        await writeFile(path.join(root,'analysis/rea',`${call.output || i+'-'+call.name}.json`),JSON.stringify(result,null,2));
        console.log(`Saved ${call.output || call.name}; error=${result.isError || false}`);
    }
}
if(process.argv.includes('--interactive')) {
    console.log('Ready for one JSON tool call per line. Send {"done":true} to close.');
    for await(const line of readline.createInterface({input:process.stdin})) {
        const call=JSON.parse(line);
        if(call.done) break;
        const result=await client.callTool({name:call.name,arguments:call.arguments},{timeout:360000});
        await writeFile(path.join(root,'analysis/rea',`${call.output}.json`),JSON.stringify(result,null,2));
        console.log(`Saved ${call.output}; error=${result.isError || false}`);
    }
}
await client.close();
