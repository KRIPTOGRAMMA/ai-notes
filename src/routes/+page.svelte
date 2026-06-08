<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  async function testCreateTask() {
    const result = await invoke("create_task", {
      task: {
        title: "Тестовая задача",
        description: null,
        status: "Todo",
        priority: "Medium",
        category: "Work",
        deadline: null,
        tags: [],
      }
    });
    console.log(result);
  }
</script>

<button onclick={testCreateTask}>Создать задачу</button>