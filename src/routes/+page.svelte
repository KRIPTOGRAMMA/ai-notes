<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let tasks = [];
  
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
    await testGetTasks();
  }
  
  async function testGetTasks() {
    tasks = await invoke("get_tasks");
    console.log(tasks);
  }
</script>

<button onclick={testCreateTask}>Создать задачу</button>
<button onclick={testGetTasks}>Получить задачи</button>

<h2>Список задач</h2>

{#if tasks.length === 0}
  <p>Задач нет</p>
{:else}
  <ul>
    {#each tasks as task}
      <li>
        <strong>{task.title}</strong>
        <br />
        Статус: {task.status}
      </li>
    {/each}
  </ul>
{/if}