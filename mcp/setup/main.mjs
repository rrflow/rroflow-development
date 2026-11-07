import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { createSetupServer } from './index.mjs';

await createSetupServer().connect(new StdioServerTransport());
