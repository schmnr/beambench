import type { AppCommandId } from './appCommandIds';
import contractItems from './toolsMenuContract.json';

export interface ToolsMenuContractItem {
  label: string;
  commandId: AppCommandId;
  shortcut?: string;
  parent?: 'Draw Shape' | 'Boolean Operations' | 'Image Options' | 'Transform';
}

export const TOOLS_MENU_CONTRACT = contractItems as readonly ToolsMenuContractItem[];
